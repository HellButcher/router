use router_types::coordinate::LatLon;

use crate::{data::Versioned, tablefile::TableData};

use super::SimpleHeader;

impl TableData for LatLon {
    type Header = SimpleHeader<LatLon>;
}

impl Versioned for LatLon {
    const VERSION: u32 = 1;
}
