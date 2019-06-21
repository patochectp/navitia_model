// Copyright 2017 Kisio Digital and/or its affiliates.
//
// This program is free software: you can redistribute it and/or
// modify it under the terms of the GNU General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
// General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see
// <http://www.gnu.org/licenses/>.

//! See function merge_fare

use crate::collection::{Collection, CollectionWithId, Id};
use crate::model::{Collections, Model};
use crate::objects::{ObjectType, RestrictionType, TicketUsePerimeter, TicketUseRestriction};
use crate::read_utils;
use crate::read_utils::{FileHandler, ZipHandler};
use crate::utils::{make_collection, make_collection_with_id, Report, ReportType};
use crate::Result;
use csv;
use failure::{bail, format_err};
use log::info;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};

fn fill_collection_with_id<T, R: std::io::Seek + std::io::Read>(
    zip_handler: &mut ZipHandler<R>,
    file_name: &str,
) -> Result<CollectionWithId<T>>
where
    T: Id<T>,
    for<'de> T: serde::Deserialize<'de>,
{
    let (reader, file_path) = zip_handler.get_file_if_exists(file_name)?;
    match reader {
        None => {
            bail!("{} not found", file_name);
        }
        Some(_reader) => {
            info!("Reading {}", file_name);
            make_collection_with_id(&file_path, file_name)
        }
    }
}

fn fill_collection<T, R: std::io::Seek + std::io::Read>(
    zip_handler: &mut ZipHandler<R>,
    file_name: &str,
) -> Result<Collection<T>>
where
    for<'de> T: serde::Deserialize<'de>,
{
    let (reader, file_path) = zip_handler.get_file_if_exists(file_name)?;
    match reader {
        None => {
            bail!("{} not found", file_name);
        }
        Some(_reader) => {
            info!("Reading {}", file_name);
            make_collection(&file_path, file_name)
        }
    }
}

fn fill_ticket_use_perimeters<R: std::io::Seek + std::io::Read>(
    collections: &Collections,
    zip_handler: &mut ZipHandler<R>,
    file_name: &str,
    report: &mut Report,
) -> Result<Collection<TicketUsePerimeter>> {
    let (reader, _file_path) = zip_handler.get_file_if_exists(file_name)?;
    match reader {
        None => {
            bail!("{} not found", file_name);
        }
        Some(reader) => {
            info!("Reading {}", file_name);
            let mut ticket_use_perimeters: Vec<TicketUsePerimeter> = vec![];
            let mut rdr = csv::Reader::from_reader(reader);
            for ticket_use_perimeter in rdr.deserialize() {
                let ticket_use_perimeter: TicketUsePerimeter = skip_fail!(ticket_use_perimeter
                    .map_err(|e| format_err!("Problem reading {:?}: {}", file_name, e)));
                match ticket_use_perimeter.object_type {
                    ObjectType::Network => {
                        if collections
                            .networks
                            .get(&ticket_use_perimeter.object_id)
                            .is_some()
                        {
                            report.add_error(
                                format!("network_id {} not found", ticket_use_perimeter.object_id),
                                ReportType::ObjectNotFound,
                            );
                        } else {
                            ticket_use_perimeters.push(ticket_use_perimeter);
                        }
                    }
                    ObjectType::Line => {}
                    _ => {
                        if collections
                            .lines
                            .get(&ticket_use_perimeter.object_id)
                            .is_some()
                        {
                            report.add_error(
                                format!("line_id {} not found", ticket_use_perimeter.object_id),
                                ReportType::ObjectNotFound,
                            );
                        } else {
                            ticket_use_perimeters.push(ticket_use_perimeter);
                        }
                    }
                }
            }
            Ok(Collection::new(ticket_use_perimeters))
        }
    }
}

fn fill_ticket_use_restrictions<R: std::io::Seek + std::io::Read>(
    collections: &Collections,
    zip_handler: &mut ZipHandler<R>,
    file_name: &str,
    report: &mut Report,
) -> Result<Collection<TicketUseRestriction>> {
    let mut ticket_use_restrictions: Vec<TicketUseRestriction> = vec![];
    let (reader, _file_path) = zip_handler.get_file_if_exists(file_name)?;
    match reader {
        None => {
            info!("{} not found", file_name);
            Ok(Collection::new(ticket_use_restrictions))
        }
        Some(reader) => {
            info!("Reading {}", file_name);
            let mut rdr = csv::Reader::from_reader(reader);
            for ticket_use_restriction in rdr.deserialize() {
                let ticket_use_restriction: TicketUseRestriction =
                    skip_fail!(ticket_use_restriction.map_err(|e| format_err!(
                        "Problem reading {:?}: {}",
                        file_name,
                        e
                    )));
                match ticket_use_restriction.restriction_type {
                    RestrictionType::OriginDestination => {
                        if collections
                            .stop_areas
                            .get(&ticket_use_restriction.use_origin)
                            .is_none()
                        {
                            report.add_error(
                                format!("origin {} not found", ticket_use_restriction.use_origin),
                                ReportType::ObjectNotFound,
                            );
                            continue;
                        }
                        if collections
                            .stop_areas
                            .get(&ticket_use_restriction.use_destination)
                            .is_none()
                        {
                            report.add_error(
                                format!(
                                    "destination {} not found",
                                    ticket_use_restriction.use_destination
                                ),
                                ReportType::ObjectNotFound,
                            );
                            continue;
                        }
                        ticket_use_restrictions.push(ticket_use_restriction);
                    }
                    RestrictionType::Zone => {
                        ticket_use_restrictions.push(ticket_use_restriction);
                    }
                }
            }
            Ok(Collection::new(ticket_use_restrictions))
        }
    }
}

fn sanitize_tickets(mut collections: Collections) -> Result<Collections> {
    let ticket_ids = collections
        .ticket_use_perimeters
        .values()
        .map(|ticket| ticket.ticket_use_id.clone())
        .chain(
            collections
                .ticket_use_restrictions
                .values()
                .map(|ticket| ticket.ticket_use_id.clone()),
        )
        .collect::<Vec<String>>();

    collections
        .ticket_prices
        .retain(|ticket| ticket_ids.contains(&ticket.ticket_id));
    Ok(collections)
}

fn read_farev2<P: AsRef<Path>>(
    mut collections: Collections,
    fare_file: P,
    report: &mut Report,
) -> Result<Collections> {
    info!("Reading fare v2 files.");

    let reader = File::open(fare_file.as_ref())?;
    let mut zip_handler = read_utils::ZipHandler::new(reader, fare_file)?;

    collections.tickets = fill_collection_with_id(&mut zip_handler, "tickets.txt")?;
    collections.ticket_uses = fill_collection_with_id(&mut zip_handler, "ticket_uses.txt")?;
    collections.ticket_prices = fill_collection(&mut zip_handler, "ticket_prices.txt")?;
    collections.ticket_use_perimeters = fill_ticket_use_perimeters(
        &collections,
        &mut zip_handler,
        "ticket_use_perimeters.txt",
        report,
    )?;
    collections.ticket_use_restrictions = fill_ticket_use_restrictions(
        &collections,
        &mut zip_handler,
        "ticket_use_perimeters.txt",
        report,
    )?;
    sanitize_tickets(collections)
}

///merge farev2 into ntfs
pub fn merge_fare(
    collections: Collections,
    fare_paths: PathBuf,
    report_path: PathBuf,
) -> Result<Model> {
    let mut report = Report::default();
    let collections = read_farev2(collections, fare_paths, &mut report)?;
    let serialized_report = serde_json::to_string_pretty(&report)?;
    fs::write(report_path, serialized_report)?;
    Model::new(collections)
}
