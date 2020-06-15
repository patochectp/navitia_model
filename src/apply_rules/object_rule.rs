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
    objects::{Line, ObjectType as ModelObjectType, VehicleJourney},
    report::{Report, TransitModelReportCategory},
    Result,
};
use failure::format_err;
use log::info;
use relational_types::IdxSet;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;
use std::{collections::HashMap, convert::TryFrom, fs::File, path::Path};
use typed_index_collection::{CollectionWithId, Id};

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
    fn check(&self, id_key: &'_ str) -> Result<&str> {
        let id = self
            .properties
            .get(id_key)
            .ok_or_else(|| format_err!("Key \"{}\" is required", id_key))?
            .as_str()
            .ok_or_else(|| format_err!("Value for \"{}\" must be filled in", id_key))?;

        Ok(id)
    }

    fn regroup<T, F>(
        &self,
        id: &'_ str,
        collection: &CollectionWithId<T>,
        report: &mut Report<TransitModelReportCategory>,
        mut update: F,
    ) -> Result<bool>
    where
        F: FnMut(&str, &str) -> bool,
    {
        let mut changed = false;
        for grouped_id in &self.grouped_from {
            if !collection.contains_id(&grouped_id) {
                report.add_error(
                    format!("The identifier \"{}\" doesn't exist, and therefore cannot be regrouped in \"{}\"", grouped_id, id),
                    TransitModelReportCategory::ObjectNotFound,
                );
            } else {
                changed = update(id, grouped_id) || changed;
            }
        }
        Ok(changed)
    }
    fn apply<T, F>(
        &self,
        id_key: &'_ str,
        collection: &mut CollectionWithId<T>,
        report: &mut Report<TransitModelReportCategory>,
        update: F,
    ) -> Result<()>
    where
        T: DeserializeOwned + Id<T>,
        F: FnMut(&str, &str) -> bool,
    {
        let id = self.check(id_key)?;
        if !collection.contains_id(id) {
            collection.push(serde_json::from_value(self.properties.clone())?)?;
        }

        let rule_applied = self.regroup(id, collection, report, update)?;
        if !rule_applied {
            report.add_error(
                format!("The rule on identifier \"{}\" was not applied", id),
                TransitModelReportCategory::ObjectNotFound,
            );
        }
        collection.retain(|object| !self.grouped_from.contains(&String::from(object.id())));
        Ok(())
    }
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
            info!("Checking networks rules.");
            for rule in networks_rules {
                let networks = &mut collections.networks;
                let lines = &mut collections.lines;
                let ticket_use_perimeters = &mut collections.ticket_use_perimeters;
                let regroup_update = |network_id: &str, removed_id: &str| {
                    if let Some(line_indexes) = lines_by_network.get(removed_id) {
                        for line_idx in line_indexes {
                            lines.index_mut(*line_idx).network_id = network_id.to_string();
                        }
                        ticket_use_perimeters
                            .values_mut()
                            .filter(|ticket| ticket.object_type == ModelObjectType::Network)
                            .filter(|ticket| ticket.object_id == removed_id)
                            .for_each(|mut ticket| ticket.object_id = network_id.to_string());
                        true
                    } else {
                        false
                    }
                };
                rule.apply("network_id", networks, report, regroup_update)?;
            }
        };
        if let (Some(commercial_modes_rules), Some(lines_by_commercial_mode)) = (
            &self.configuration.commercial_modes_rules,
            &self.lines_by_commercial_mode,
        ) {
            info!("Checking commercial modes rules.");
            for rule in commercial_modes_rules {
                let commercial_modes = &mut collections.commercial_modes;
                let lines = &mut collections.lines;
                let regroup_update = |commercial_mode_id: &str, removed_id: &str| {
                    if let Some(line_indexes) = lines_by_commercial_mode.get(removed_id) {
                        for line_idx in line_indexes {
                            lines.index_mut(*line_idx).commercial_mode_id =
                                commercial_mode_id.to_string();
                        }
                        true
                    } else {
                        false
                    }
                };
                rule.apply(
                    "commercial_mode_id",
                    commercial_modes,
                    report,
                    regroup_update,
                )?;
            }
        };
        if let (Some(physical_modes_rules), Some(vjs_by_physical_mode)) = (
            &self.configuration.physical_modes_rules,
            &self.vjs_by_physical_mode,
        ) {
            info!("Checking physical modes rules.");
            for rule in physical_modes_rules {
                let physical_modes = &mut collections.physical_modes;
                let vehicle_journeys = &mut collections.vehicle_journeys;
                let regroup_update = |physical_mode_id: &str, removed_id: &str| {
                    if let Some(vehicle_journey_indexes) = vjs_by_physical_mode.get(removed_id) {
                        for vehicle_journey_idx in vehicle_journey_indexes {
                            vehicle_journeys
                                .index_mut(*vehicle_journey_idx)
                                .physical_mode_id = physical_mode_id.to_string();
                        }
                        true
                    } else {
                        false
                    }
                };
                rule.apply("physical_mode_id", physical_modes, report, regroup_update)?;
            }
        };
        Ok(())
    }
}
