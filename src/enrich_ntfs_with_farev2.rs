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

use crate::model::Collections;
use crate::utils::Report;
use crate::Result;
use std::path::PathBuf;

///merge farev2 into ntfs
pub fn merge_fare(
    mut collections: Collections,
    fare_paths: PathBuf,
    report_path: PathBuf,
) -> Result<Collections> {
    let mut _report = Report::default();
    Ok(collections)
}
