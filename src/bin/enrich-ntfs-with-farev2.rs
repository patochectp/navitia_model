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

use chrono::NaiveDateTime;
use log::info;
use std::path::PathBuf;
use structopt::StructOpt;
use transit_model::Result;

#[derive(Debug, StructOpt)]
#[structopt(name = "enrich_ntfs_with_farev2", about = "Merge farev2 into ntfs")]

struct Opt {
    /// input directory.
    #[structopt(short = "i", long = "input", parse(from_os_str))]
    input: PathBuf,

    /// fare compressed file
    #[structopt(short, long = "fare", parse(from_os_str))]
    farev2: PathBuf,

    /// output report file path
    #[structopt(short = "r", long = "report", parse(from_os_str))]
    report: PathBuf,

    /// output directory
    #[structopt(short = "o", long = "output", parse(from_os_str))]
    output: PathBuf,

    /// current datetime
    #[structopt(
        short = "x",
        long,
        parse(try_from_str),
        raw(default_value = "&transit_model::CURRENT_DATETIME")
    )]
    current_datetime: NaiveDateTime,
}

fn run() -> Result<()> {
    info!("Launching enrich-ntfs-with-farev2...");

    let opt = Opt::from_args();

    let objects = transit_model::ntfs::read(opt.input)?;
    let collections = objects.into_collections();

    let new_model =
        transit_model::enrich_ntfs_with_farev2::merge_fare(collections, opt.farev2, opt.report)?;

    transit_model::ntfs::write(&new_model, opt.output, opt.current_datetime)?;
    Ok(())
}

fn main() {
    env_logger::init();
    if let Err(err) = run() {
        for cause in err.iter_chain() {
            eprintln!("{}", cause);
        }
        std::process::exit(1);
    }
}
