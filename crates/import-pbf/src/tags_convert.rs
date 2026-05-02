use router_storage::data::{
    attrib::{HighwayClass, NodeFlags, SurfaceQuality, WayFlags},
    dim_restriction::DimRestriction,
    edge::EdgeFlags,
};

use crate::tags::{NodeTags, WayTags};

impl WayTags<'_> {
    pub fn highway_class(&self) -> HighwayClass {
        let highway = self.highway;
        let service = self.service;
        use super::tags::Highway as H;
        use super::tags::Service as S;
        if matches!(highway, Some(H::service)) {
            return match service {
                Some(S::driveway) => HighwayClass::ServiceDriveway,
                Some(S::parking_aisle) => HighwayClass::ServiceParkingAisle,
                Some(S::alley) => HighwayClass::ServiceAlley,
                _ => HighwayClass::Service,
            };
        }
        match highway {
            Some(H::motorway) => HighwayClass::Motorway,
            Some(H::trunk) => HighwayClass::Trunk,
            Some(H::primary) => HighwayClass::Primary,
            Some(H::secondary) => HighwayClass::Secondary,
            Some(H::tertiary) => HighwayClass::Tertiary,
            Some(H::motorway_link) => HighwayClass::MotorwayLink,
            Some(H::trunk_link) => HighwayClass::TrunkLink,
            Some(H::primary_link) => HighwayClass::PrimaryLink,
            Some(H::secondary_link) => HighwayClass::SecondaryLink,
            Some(H::tertiary_link) => HighwayClass::TertiaryLink,
            Some(H::unclassified) => HighwayClass::Unclassified,
            Some(H::residential) => HighwayClass::Residential,
            Some(H::living_street) => HighwayClass::LivingStreet,
            Some(H::service) => HighwayClass::Service,
            Some(H::track) => HighwayClass::Track,
            Some(H::road) => HighwayClass::Road,
            Some(H::pedestrian) => HighwayClass::Pedestrian,
            Some(H::footway) => HighwayClass::Footway,
            Some(H::cycleway) => HighwayClass::Cycleway,
            Some(H::path) => HighwayClass::Path,
            Some(H::bridleway) => HighwayClass::Bridleway,
            _ => HighwayClass::Unknown,
        }
    }

    pub fn is_oneway_reverse(&self) -> bool {
        use super::tags::{Conditional, OneWay};
        matches!(&self.oneway, Conditional::Simple(OneWay::reverse))
    }

    /// WayFlags for metadata shared across all edges of the same OSM way.
    pub fn derive_way_flags(&self) -> WayFlags {
        use super::tags::{Conditional, OneWay};
        let mut flags = WayFlags::empty();
        if matches!(
            &self.oneway,
            Conditional::Simple(OneWay::yes | OneWay::reverse)
        ) || self.junction.is_some_and(|j| j.implies_oneway())
        {
            flags |= WayFlags::ONEWAY;
        }
        if self.toll == Some(true) {
            flags |= WayFlags::TOLL;
        }
        if self.tunnel {
            flags |= WayFlags::TUNNEL;
        }
        if self.bridge {
            flags |= WayFlags::BRIDGE;
        }
        flags
    }

    /// Access flags for one traversal direction, derived from highway class and access tags.
    ///
    /// `is_forward`: true for the forward (node[0]→node[n-1]) direction.
    /// Items with `direction == both` always apply; items with `direction == forward` / `backward`
    /// only apply when the traversal direction matches.
    pub fn derive_directional_access(&self, highway: HighwayClass, is_forward: bool) -> EdgeFlags {
        use super::tags::{Conditional as Cond, Direction, Mode};

        let mut flags = EdgeFlags::empty();

        match highway {
            HighwayClass::Footway
            | HighwayClass::Pedestrian
            | HighwayClass::Cycleway
            | HighwayClass::Path => {
                flags |= EdgeFlags::NO_MOTOR | EdgeFlags::NO_HGV;
            }
            HighwayClass::Bridleway => {
                flags |= EdgeFlags::NO_MOTOR | EdgeFlags::NO_HGV | EdgeFlags::NO_BICYCLE;
            }
            HighwayClass::Motorway | HighwayClass::MotorwayLink => {
                flags |= EdgeFlags::NO_BICYCLE | EdgeFlags::NO_FOOT;
            }
            _ => {}
        }

        if self.motorroad {
            flags |= EdgeFlags::NO_BICYCLE | EdgeFlags::NO_FOOT;
        }

        // `Simple` access has no direction — applies to both.
        if let Cond::Simple(a) = &self.access
            && a.is_excluded()
        {
            flags |= EdgeFlags::NO_MOTOR
                | EdgeFlags::NO_HGV
                | EdgeFlags::NO_BICYCLE
                | EdgeFlags::NO_FOOT;
        }

        if let Cond::Multi(items) = &self.access {
            let dir_matches = |d: Direction| match d {
                Direction::forward => is_forward,
                Direction::backward => !is_forward,
                Direction::both => true,
                // Lane-level left/right — not supported at way level.
                Direction::left | Direction::right | Direction::unknown => false,
            };

            for item in items
                .iter()
                .filter(|i| dir_matches(i.direction) && i.value.is_excluded())
            {
                match item.mode {
                    Mode::default => {
                        flags |= EdgeFlags::NO_MOTOR
                            | EdgeFlags::NO_HGV
                            | EdgeFlags::NO_BICYCLE
                            | EdgeFlags::NO_FOOT;
                    }
                    Mode::vehicle => {
                        flags |= EdgeFlags::NO_MOTOR | EdgeFlags::NO_HGV | EdgeFlags::NO_BICYCLE;
                    }
                    Mode::motor_vehicle => {
                        flags |= EdgeFlags::NO_MOTOR | EdgeFlags::NO_HGV;
                    }
                    Mode::motorcar
                    | Mode::motorcycle
                    | Mode::moped
                    | Mode::mofa
                    | Mode::motorhome => {
                        flags |= EdgeFlags::NO_MOTOR;
                    }
                    Mode::hgv | Mode::goods | Mode::coach | Mode::tourist_bus => {
                        flags |= EdgeFlags::NO_HGV;
                    }
                    Mode::bicycle => flags |= EdgeFlags::NO_BICYCLE,
                    Mode::foot => flags |= EdgeFlags::NO_FOOT,
                    _ => {}
                }
            }
            for item in items
                .iter()
                .filter(|i| dir_matches(i.direction) && !i.value.is_excluded())
            {
                match item.mode {
                    Mode::vehicle => {
                        flags &= !(EdgeFlags::NO_MOTOR | EdgeFlags::NO_HGV | EdgeFlags::NO_BICYCLE);
                    }
                    Mode::motor_vehicle => {
                        flags &= !(EdgeFlags::NO_MOTOR | EdgeFlags::NO_HGV);
                    }
                    Mode::motorcar
                    | Mode::motorcycle
                    | Mode::moped
                    | Mode::mofa
                    | Mode::motorhome => {
                        flags &= !EdgeFlags::NO_MOTOR;
                    }
                    Mode::hgv | Mode::goods | Mode::coach | Mode::tourist_bus => {
                        flags &= !EdgeFlags::NO_HGV;
                    }
                    Mode::bicycle => flags &= !EdgeFlags::NO_BICYCLE,
                    Mode::foot => flags &= !EdgeFlags::NO_FOOT,
                    _ => {}
                }
            }
        }

        flags
    }

    pub fn derive_dim_restriction(&self) -> DimRestriction {
        let height_dm = min_dim(self.raw_max_height_physical, self.raw_max_height)
            .map(|m| (m * 10.0).round() as u8)
            .unwrap_or(0);
        let width_dm = min_dim(self.raw_max_width_physical, self.raw_max_width)
            .map(|m| (m * 10.0).round() as u8)
            .unwrap_or(0);
        let weight_250kg = self
            .raw_max_weight
            .and_then(parse_weight_t)
            .map(|t| (t * 4.0).round() as u8)
            .unwrap_or(0);
        let length_dm = self
            .raw_max_length
            .and_then(parse_dim_m)
            .map(|m| (m * 10.0).round() as u8)
            .unwrap_or(0);
        DimRestriction {
            max_height_dm: height_dm,
            max_width_dm: width_dm,
            max_length_dm: length_dm,
            max_weight_250kg: weight_250kg,
        }
    }

    pub fn is_bicycle_contraflow(&self) -> bool {
        use super::tags::{Conditional, Mode, OneWay};
        if let Conditional::Multi(items) = &self.oneway {
            items
                .iter()
                .any(|i| i.mode == Mode::bicycle && matches!(i.value, OneWay::no))
        } else {
            false
        }
    }

    pub fn surface_quality(&self) -> SurfaceQuality {
        use super::tags::{Smoothness, Surface, TrackType};

        if let Some(s) = self.smoothness {
            return match s {
                Smoothness::excellent => SurfaceQuality::Excellent,
                Smoothness::good | Smoothness::intermediate => SurfaceQuality::Good,
                Smoothness::bad => SurfaceQuality::Bad,
                Smoothness::very_bad => SurfaceQuality::VeryBad,
                Smoothness::horrible | Smoothness::very_horrible => SurfaceQuality::Horrible,
                Smoothness::impassable => SurfaceQuality::Impassable,
                Smoothness::unknown => SurfaceQuality::Unknown,
            };
        }

        if let Some(t) = self.tracktype {
            return match t {
                TrackType::grade1 => SurfaceQuality::Good,
                TrackType::grade2 => SurfaceQuality::Intermediate,
                TrackType::grade3 => SurfaceQuality::Bad,
                TrackType::grade4 => SurfaceQuality::VeryBad,
                TrackType::grade5 => SurfaceQuality::Horrible,
                TrackType::unknown => SurfaceQuality::Unknown,
            };
        }

        if let Some(s) = self.surface {
            return match s {
                Surface::asphalt | Surface::concrete | Surface::metal | Surface::rubber => {
                    SurfaceQuality::Excellent
                }
                Surface::paved | Surface::paving_stones => SurfaceQuality::Good,
                Surface::cobblestone
                | Surface::wood
                | Surface::stepping_stones
                | Surface::compacted => SurfaceQuality::Intermediate,
                Surface::unpaved => SurfaceQuality::Bad,
                Surface::gravel | Surface::ground => SurfaceQuality::VeryBad,
                Surface::grass => SurfaceQuality::Horrible,
                Surface::sand | Surface::ice => SurfaceQuality::Impassable,
                Surface::unknown => SurfaceQuality::Unknown,
            };
        }

        SurfaceQuality::Unknown
    }
}

impl NodeTags<'_> {
    pub fn derive_node_flags(&self) -> NodeFlags {
        use super::tags::{Barrier, Conditional as Cond, Mode, NodeHighway};
        let mut flags = NodeFlags::empty();

        if let Some(barrier) = self.barrier {
            match barrier {
                Barrier::bollard | Barrier::gate => {
                    flags |= NodeFlags::NO_MOTOR | NodeFlags::NO_HGV;
                }
                Barrier::kissing_gate => {
                    flags |= NodeFlags::NO_MOTOR | NodeFlags::NO_HGV | NodeFlags::NO_BICYCLE;
                }
                Barrier::cycle_barrier => {
                    flags |= NodeFlags::NO_BICYCLE;
                }
                Barrier::unknown => {}
            }
        }

        if let Cond::Simple(a) = &self.access
            && a.is_excluded()
        {
            flags |= NodeFlags::NO_MOTOR
                | NodeFlags::NO_HGV
                | NodeFlags::NO_BICYCLE
                | NodeFlags::NO_FOOT;
        }
        if let Cond::Multi(items) = &self.access {
            for item in items.iter().filter(|i| i.value.is_excluded()) {
                match item.mode {
                    Mode::default => {
                        flags |= NodeFlags::NO_MOTOR
                            | NodeFlags::NO_HGV
                            | NodeFlags::NO_BICYCLE
                            | NodeFlags::NO_FOOT;
                    }
                    Mode::vehicle => {
                        flags |= NodeFlags::NO_MOTOR | NodeFlags::NO_HGV | NodeFlags::NO_BICYCLE;
                    }
                    Mode::motor_vehicle => flags |= NodeFlags::NO_MOTOR | NodeFlags::NO_HGV,
                    Mode::motorcar
                    | Mode::motorcycle
                    | Mode::moped
                    | Mode::mofa
                    | Mode::motorhome => {
                        flags |= NodeFlags::NO_MOTOR;
                    }
                    Mode::hgv | Mode::goods | Mode::coach | Mode::tourist_bus => {
                        flags |= NodeFlags::NO_HGV;
                    }
                    Mode::bicycle => flags |= NodeFlags::NO_BICYCLE,
                    Mode::foot => flags |= NodeFlags::NO_FOOT,
                    _ => {}
                }
            }
            for item in items.iter().filter(|i| !i.value.is_excluded()) {
                match item.mode {
                    Mode::vehicle => {
                        flags &= !(NodeFlags::NO_MOTOR | NodeFlags::NO_HGV | NodeFlags::NO_BICYCLE);
                    }
                    Mode::motor_vehicle => flags &= !(NodeFlags::NO_MOTOR | NodeFlags::NO_HGV),
                    Mode::motorcar
                    | Mode::motorcycle
                    | Mode::moped
                    | Mode::mofa
                    | Mode::motorhome => {
                        flags &= !NodeFlags::NO_MOTOR;
                    }
                    Mode::hgv | Mode::goods | Mode::coach | Mode::tourist_bus => {
                        flags &= !NodeFlags::NO_HGV;
                    }
                    Mode::bicycle => flags &= !NodeFlags::NO_BICYCLE,
                    Mode::foot => flags &= !NodeFlags::NO_FOOT,
                    _ => {}
                }
            }
        }

        if self
            .highway
            .is_some_and(|h| matches!(h, NodeHighway::traffic_signals))
        {
            flags |= NodeFlags::TRAFFIC_SIGNALS;
        }

        if self.toll == Some(true) {
            flags |= NodeFlags::TOLL;
        }

        flags
    }
}

fn parse_dim_m(v: &str) -> Option<f32> {
    let v = v.trim();
    if let Some(rest) = v.find('\'').map(|p| (v[..p].trim(), v[p + 1..].trim())) {
        let feet: f32 = rest.0.parse().ok()?;
        let inches: f32 = rest
            .1
            .strip_suffix('"')
            .unwrap_or(rest.1)
            .trim_end()
            .parse()
            .unwrap_or(0.0);
        return Some(feet * 0.3048 + inches * 0.0254);
    }
    let v = v.strip_suffix('m').unwrap_or(v).trim_end();
    v.parse::<f32>().ok()
}

fn parse_weight_t(v: &str) -> Option<f32> {
    let v = v.trim();
    let v = v.strip_suffix('t').unwrap_or(v).trim_end();
    v.parse::<f32>().ok()
}

fn min_dim(a: Option<&str>, b: Option<&str>) -> Option<f32> {
    match (a.and_then(parse_dim_m), b.and_then(parse_dim_m)) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    }
}
