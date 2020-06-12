// Copyright (C) 2017 Kisio Digital and/or its affiliates.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU Affero General Public License as published by the
// Free Software Foundation, version 3.

// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more
// details.

// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>

use crate::{
    model::{Collections, Model},
    objects::{
        CommercialMode, Line, Network, ObjectType as ModelObjectType, PhysicalMode, VehicleJourney,
    },
    report::{Report, TransitModelReportCategory},
    Result,
};
use failure::format_err;
use log::info;
use relational_types::IdxSet;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    convert::TryFrom,
    fs::File,
    path::Path,
};
use typed_index_collection::CollectionWithId;

#[derive(Debug, Deserialize)]
pub struct ObjectProperties {
    properties: Value,
    #[serde(default)]
    grouped_from: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ObjectRuleConfiguration {
    #[serde(rename = "networks")]
    pub networks_rules: Option<Vec<ObjectProperties>>,
    #[serde(rename = "commercial_modes")]
    pub commercial_modes_rules: Option<Vec<ObjectProperties>>,
    #[serde(rename = "physical_modes")]
    pub physical_modes_rules: Option<Vec<ObjectProperties>>,
}

impl TryFrom<&Path> for ObjectRuleConfiguration {
    type Error = failure::Error;
    fn try_from(path: &Path) -> Result<Self> {
        info!("Reading object rules");
        File::open(path)
            .map_err(|e| format_err!("{}", e))
            .and_then(|file| {
                serde_json::from_reader::<_, ObjectRuleConfiguration>(file)
                    .map_err(|e| format_err!("{}", e))
            })
    }
}

#[derive(Debug)]
pub struct ObjectRule {
    configuration: ObjectRuleConfiguration,
    lines_by_network: Option<HashMap<String, IdxSet<Line>>>,
    lines_by_commercial_mode: Option<HashMap<String, IdxSet<Line>>>,
    vjs_by_physical_mode: Option<HashMap<String, IdxSet<VehicleJourney>>>,
}

impl ObjectRule {
    pub(crate) fn new(path: &Path, model: &Model) -> Result<Self> {
        let configuration = ObjectRuleConfiguration::try_from(path)?;
        let lines_by_network = if configuration.networks_rules.is_some() {
            Some(
                model
                    .networks
                    .iter()
                    .filter_map(|(idx, obj)| {
                        let lines = model.get_corresponding_from_idx(idx);
                        if lines.is_empty() {
                            None
                        } else {
                            Some((obj.id.clone(), lines))
                        }
                    })
                    .collect(),
            )
        } else {
            None
        };
        let lines_by_commercial_mode = if configuration.commercial_modes_rules.is_some() {
            Some(
                model
                    .commercial_modes
                    .iter()
                    .filter_map(|(idx, obj)| {
                        let lines = model.get_corresponding_from_idx(idx);
                        if lines.is_empty() {
                            None
                        } else {
                            Some((obj.id.clone(), lines))
                        }
                    })
                    .collect(),
            )
        } else {
            None
        };
        let vjs_by_physical_mode = if configuration.physical_modes_rules.is_some() {
            Some(
                model
                    .physical_modes
                    .iter()
                    .filter_map(|(idx, obj)| {
                        let vjs = model.get_corresponding_from_idx(idx);
                        if vjs.is_empty() {
                            None
                        } else {
                            Some((obj.id.clone(), vjs))
                        }
                    })
                    .collect(),
            )
        } else {
            None
        };
        let object_rule = ObjectRule {
            configuration,
            lines_by_network,
            lines_by_commercial_mode,
            vjs_by_physical_mode,
        };
        Ok(object_rule)
    }
}

impl ObjectProperties {
    fn check(
        &self,
        id_key: &'_ str,
        report: &mut Report<TransitModelReportCategory>,
    ) -> Result<Option<&str>> {
        let id = self
            .properties
            .get(id_key)
            .ok_or_else(|| format_err!("Key \"{}\" is required", id_key))?
            .as_str()
            .ok_or_else(|| format_err!("Value for \"{}\" must be filled in", id_key))?;

        if self.grouped_from.is_empty() {
            report.add_error(
                format!(
                    "The list to group by \"{}\" is empty for consolidation in \"{}\"",
                    id_key, id
                ),
                TransitModelReportCategory::ObjectNotFound,
            );
            Ok(None)
        } else {
            Ok(Some(id))
        }
    }

    fn regroup<T, F>(
        &self,
        collection: &CollectionWithId<T>,
        report: &mut Report<TransitModelReportCategory>,
        mut f: F,
    ) -> Result<bool>
    where
        F: FnMut(&str) -> bool,
    {
        let mut changed = false;
        for regroup_id in &self.grouped_from {
            if !collection.contains_id(&regroup_id) {
                report.add_error(
                    format!("The identifier \"{}\" to regroup doesn't exist", regroup_id),
                    TransitModelReportCategory::ObjectNotFound,
                );
            } else {
                changed = f(regroup_id) || changed;
            }
        }
        Ok(changed)
    }
}

fn check_and_apply_physical_modes_rules(
    report: &mut Report<TransitModelReportCategory>,
    collections: &mut Collections,
    physical_modes_rules: &[ObjectProperties],
    vjs_by_physical_mode: &HashMap<String, IdxSet<VehicleJourney>>,
) -> Result<()> {
    info!("Checking physical modes rules.");
    let mut new_physical_modes: Vec<PhysicalMode> = vec![];

    for pyr in physical_modes_rules {
        if let Some(physical_mode_id) = pyr.check("physical_mode_id", report)? {
            if !collections.physical_modes.contains_id(physical_mode_id) {
                new_physical_modes.push(serde_json::from_value(pyr.properties.clone())?)
            }

            let physical_modes = &collections.physical_modes;
            let c_vehicle_journeys = &mut collections.vehicle_journeys;
            let physical_mode_rule = pyr.regroup(physical_modes, report, |regroup_id| {
                if let Some(vehicle_journeys) = vjs_by_physical_mode.get(regroup_id) {
                    for vehicle_journey_idx in vehicle_journeys {
                        c_vehicle_journeys
                            .index_mut(*vehicle_journey_idx)
                            .physical_mode_id = physical_mode_id.to_string();
                    }
                    true
                } else {
                    false
                }
            })?;
            if !physical_mode_rule {
                report.add_error(
                    format!(
                        "The rule on the \"{}\" physical mode was not applied",
                        physical_mode_id
                    ),
                    TransitModelReportCategory::ObjectNotFound,
                );
            }
        }
        collections
            .physical_modes
            .retain(|physical_mode| !pyr.grouped_from.contains(&physical_mode.id));
    }

    collections.physical_modes.extend(new_physical_modes);

    Ok(())
}

fn check_and_apply_commercial_modes_rules(
    report: &mut Report<TransitModelReportCategory>,
    collections: &mut Collections,
    commercial_modes_rules: &[ObjectProperties],
    lines_by_commercial_mode: &HashMap<String, IdxSet<Line>>,
) -> Result<()> {
    info!("Checking commercial modes rules.");
    let mut new_commercial_modes: Vec<CommercialMode> = vec![];

    for pyr in commercial_modes_rules {
        if let Some(commercial_mode_id) = pyr.check("commercial_mode_id", report)? {
            if !collections.commercial_modes.contains_id(commercial_mode_id) {
                new_commercial_modes.push(serde_json::from_value(pyr.properties.clone())?)
            }

            let commercial_modes = &collections.commercial_modes;
            let c_lines = &mut collections.lines;
            let commercial_mode_rule = pyr.regroup(commercial_modes, report, |regroup_id| {
                if let Some(lines) = lines_by_commercial_mode.get(regroup_id) {
                    for line_idx in lines {
                        c_lines.index_mut(*line_idx).commercial_mode_id =
                            commercial_mode_id.to_string();
                    }
                    true
                } else {
                    false
                }
            })?;
            if !commercial_mode_rule {
                report.add_error(
                    format!(
                        "The rule on the \"{}\" commercial mode was not applied",
                        commercial_mode_id
                    ),
                    TransitModelReportCategory::ObjectNotFound,
                );
            }
        }
        collections
            .commercial_modes
            .retain(|commercial_mode| !pyr.grouped_from.contains(&commercial_mode.id));
    }

    collections.commercial_modes.extend(new_commercial_modes);

    Ok(())
}

fn check_and_apply_networks_rules(
    report: &mut Report<TransitModelReportCategory>,
    collections: &mut Collections,
    networks_rules: &[ObjectProperties],
    lines_by_network: &HashMap<String, IdxSet<Line>>,
) -> Result<()> {
    info!("Checking networks rules.");
    let mut new_networks: Vec<Network> = vec![];

    for pyr in networks_rules {
        if let Some(network_id) = pyr.check("network_id", report)? {
            if !collections.networks.contains_id(network_id) {
                new_networks.push(serde_json::from_value(pyr.properties.clone())?)
            }
            let networks = &collections.networks;
            let c_lines = &mut collections.lines;
            let c_ticket_use_perimeters = &mut collections.ticket_use_perimeters;
            let network_rule = pyr.regroup(networks, report, |regroup_id| {
                if let Some(lines) = lines_by_network.get(regroup_id) {
                    for line_idx in lines {
                        c_lines.index_mut(*line_idx).network_id = network_id.to_string();
                    }

                    c_ticket_use_perimeters
                        .values_mut()
                        .filter(|ticket| ticket.object_type == ModelObjectType::Network)
                        .filter(|ticket| &ticket.object_id == regroup_id)
                        .for_each(|mut ticket| ticket.object_id = network_id.to_string());
                    true
                } else {
                    false
                }
            })?;
            if !network_rule {
                report.add_error(
                    format!("The rule on the \"{}\" network was not applied", network_id),
                    TransitModelReportCategory::ObjectNotFound,
                );
            }
        }
        collections
            .networks
            .retain(|network| !pyr.grouped_from.contains(&network.id));
    }

    collections.networks.extend(new_networks);

    Ok(())
}

impl ObjectRule {
    pub(crate) fn apply_rules(
        &self,
        collections: &mut Collections,
        report: &mut Report<TransitModelReportCategory>,
    ) -> Result<()> {
        if let (Some(networks_rules), Some(lines_by_network)) =
            (&self.configuration.networks_rules, &self.lines_by_network)
        {
            check_and_apply_networks_rules(report, collections, networks_rules, lines_by_network)?;
        };
        if let (Some(commercial_modes_rules), Some(lines_by_commercial_mode)) = (
            &self.configuration.commercial_modes_rules,
            &self.lines_by_commercial_mode,
        ) {
            check_and_apply_commercial_modes_rules(
                report,
                collections,
                commercial_modes_rules,
                lines_by_commercial_mode,
            )?;
        };
        if let (Some(physical_modes_rules), Some(vjs_by_physical_mode)) = (
            &self.configuration.physical_modes_rules,
            &self.vjs_by_physical_mode,
        ) {
            check_and_apply_physical_modes_rules(
                report,
                collections,
                physical_modes_rules,
                vjs_by_physical_mode,
            )?;
        };
        Ok(())
    }
}
