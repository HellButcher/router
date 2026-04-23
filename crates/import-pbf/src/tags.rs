macro_rules! define_tag_enum {
    (
        $(#[$m:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$mv:meta])*
                $v:ident $(| $a:ident)* $(| $b:literal)*
            ),*
            $(,)?
        }
    ) => {
        $(#[$m])*
        #[derive(Copy,Clone,Eq,PartialEq)]
        #[allow(non_camel_case_types)]
        $vis enum $name {
            $(
                $(#[$mv])*
                $v,
            )*

            unknown,
        }

        impl FromTag for $name {
            fn from_tag(v: &str) -> Self {
                match v {
                    $(stringify!($v)  $(| stringify!($a))* $(| $b)* => Self::$v,)*
                    _ => Self::unknown,
                }
            }
        }
    };
}

#[allow(unused_macros)]
macro_rules! define_tag_struct {
    (
        $(#[$m:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$mv:meta])*
                $v:vis $n:ident : $t:ty
            ),*
            $(,)?
        }
    ) => {
        $(#[$m])*
        $vis struct $name {
            $(
                $(#[$mv])*
                $v $n: $t,
            )*
        }

        impl $name {
            pub fn set_tag(&mut self, k: &str, v: &str) {
                match k {
                    $(
                        stringify!($n) => self.$n = FromTag::from_tag(v),
                    )*
                    _ => (),
                }
            }
        }
    };
}

pub trait FromTag {
    fn from_tag(v: &str) -> Self;
}

impl FromTag for bool {
    fn from_tag(v: &str) -> Self {
        !matches!(v, "no" | "false" | "unknown")
    }
}

impl<T: FromTag> FromTag for Option<T> {
    fn from_tag(v: &str) -> Self {
        Some(T::from_tag(v))
    }
}

define_tag_enum! {
    #[derive(Default)]
    pub enum Access {
        // TODO: limit to a sensible default
        #[default]
        yes | public,
        no | private | discouraged | disabled,
        permissive,
        permit,
        destination,
        delivery,
        customers | visitors,
        designated | official,
        use_sidepath,
        dismount,
        agricultural,
        forestry,
        residents | employees | students | university | members | staff,
        military,
        emergency,
    }
}

impl Access {
    pub fn is_excluded(self) -> bool {
        matches!(
            self,
            Self::no
                | Self::unknown
                | Self::permit
                | Self::military
                | Self::emergency
                | Self::forestry
                | Self::agricultural
        )
    }
}

define_tag_enum! {
    pub enum Highway {
        motorway,
        trunk,
        primary,
        secondary,
        tertiary,
        motorway_link,
        trunk_link,
        primary_link,
        secondary_link,
        tertiary_link,
        unclassified,
        residential,
        pedestrian | crossing | steps,
        living_street,
        service,
        track,
        road,
        busway | platform,
        footway,
        bridleway,
        path,
        cycleway,
        no | construction | proposed | emergency_bay | corridor | raceway | via_ferrata | abandoned | disused | escape | bus_guideway
    }
}

impl Highway {
    fn is_excluded(self) -> bool {
        matches!(self, Self::unknown | Self::no)
    }
}

define_tag_enum! {
    pub enum Smoothness {
        excellent,
        good,
        intermediate,
        bad,
        very_bad,
        horrible,
        very_horrible,
        impassable
    }
}

define_tag_enum! {
    pub enum TrackType {
        grade1,
        grade2,
        grade3,
        grade4,
        grade5,
    }
}

define_tag_enum! {
    pub enum Junction {
        /// Roundabout — implies oneway circulation.
        roundabout,
        /// Circular road — implies oneway circulation.
        circular,
        /// US jughandle ramp — does NOT imply oneway on the junction tag alone.
        jughandle,
        /// Interchange (motorway-style split).
        interchange,
    }
}

impl Junction {
    /// Returns `true` if this junction type implies a one-way direction restriction.
    pub fn implies_oneway(self) -> bool {
        matches!(self, Self::roundabout | Self::circular)
    }
}

define_tag_enum! {
    pub enum Surface {
        paved,
        asphalt,
        concrete,
        paving_stones | sett | bricks,
        cobblestone | unhewn_cobblestone,
        metal,
        wood,
        stepping_stones,
        rubber,
        // unpaved variants
        unpaved,
        compacted,
        gravel | fine_gravel | shells | pebblestone,
        ground | dirt | earth,
        grass,
        sand,
        ice | snow,
    }
}

define_tag_enum! {
    #[derive(Default)]
    pub enum OneWay {
        #[default]
        no | disabled | "false",
        yes | recommended | "true",
        reverse | "-1",
        alternating | reversivle | conditional
    }
}

define_tag_enum! {
    pub enum Barrier {
        /// Blocks motor vehicles and HGV; bicycles and pedestrians can still pass.
        bollard | block | chain | jersey_barrier | log | planter | rope | spikes,
        /// Gate-type barrier: blocks motor vehicles by default; bikes often pass.
        gate | lift_gate | swing_gate | sliding_gate | hampshire_gate | bump_gate | wicket_gate,
        /// Blocks motor, HGV, and bicycle; only foot can pass.
        kissing_gate | stile | turnstile | full_height_turnstile | sump_buster,
        /// Blocks bicycles (and motorcycles) only.
        cycle_barrier | motorcycle_barrier,
    }
}

define_tag_enum! {
    pub enum NodeHighway {
        traffic_signals,
        crossing | stop | give_way | mini_roundabout | turning_circle | turning_loop,
    }
}

define_tag_enum! {
    #[derive(Default)]
    pub enum Mode {
        #[default]
        default,

        foot,
        horse,

        vehicle,
        bicycle,
        trailer,

        motor_vehicle,
        motorcycle,
        moped,
        mofa,
        motorcar,
        motorhome,
        tourist_bus,
        coach,
        goods,
        hgv,
        //hgv_articulated,
        //bdouble,
        //agricultural
        //psv
        ////bus
        ////taxi
        ////minibus
        ////emergency
        ////hazmat
    }
}

define_tag_enum! {
    #[derive(Default)]
    pub enum Direction {
        #[default]
        both | both_ways,
        forward,
        backward,
        left,
        right,
    }
}

pub struct ConditionalItem<'s, T> {
    pub mode: Mode,
    pub direction: Direction,
    pub condition: Option<&'s str>,
    pub value: T,
}

#[derive(Default)]
pub enum Conditional<'s, T> {
    #[default]
    None,
    Simple(T),
    Multi(Vec<ConditionalItem<'s, T>>),
}

impl<'s, T: FromTag> Conditional<'s, T> {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    fn add(&mut self, mode: Mode, direction: Direction, condition: Option<&'s str>, value: T) {
        match self {
            Self::None => {
                if mode == Mode::default && direction == Direction::both && condition.is_none() {
                    *self = Self::Simple(value);
                    return;
                } else {
                    *self = Self::Multi(Vec::new());
                }
            }
            Self::Simple(_) => {
                let Self::Simple(value) = std::mem::replace(self, Self::None) else {
                    unreachable!();
                };
                *self = Self::Multi(vec![ConditionalItem {
                    mode: Mode::default,
                    direction: Direction::both,
                    condition: None,
                    value,
                }]);
            }
            _ => (),
        }

        if let Self::Multi(v) = self {
            v.push(ConditionalItem {
                mode,
                direction,
                condition,
                value,
            });
        } else {
            unreachable!();
        }
    }

    pub fn set_tag(&mut self, k: &str, v: &'s str) {
        let mut mode = Mode::default;
        let mut dir = Direction::both;
        let mut conditional = false;
        for k in k.split(':') {
            if k == "conditional" {
                conditional = true;
            } else {
                let m = Mode::from_tag(k);
                if m != Mode::unknown && m != Mode::default {
                    mode = m;
                } else {
                    let d = Direction::from_tag(k);
                    if d != Direction::unknown {
                        dir = d;
                    }
                }
            }
        }

        if conditional {
            for v in v.split(';') {
                let mut v = v.trim();
                let mut cond = None;
                if let Some(p) = v.find('@') {
                    cond = Some(v[p + 1..].trim_start());
                    v = v[0..p].trim_end();
                };
                self.add(mode, dir, cond, T::from_tag(v));
            }
        } else {
            self.add(mode, dir, None, T::from_tag(v));
        }
    }
}

#[derive(Default)]
pub struct WayTags<'s> {
    pub highway: Option<Highway>,
    pub access: Conditional<'s, Access>,
    pub oneway: Conditional<'s, OneWay>,
    pub toll: Option<bool>,
    pub surface: Option<Surface>,
    pub smoothness: Option<Smoothness>,
    pub tracktype: Option<TrackType>,
    pub junction: Option<Junction>,
    pub motorroad: bool,
    pub disused: bool,
    pub abandoned: bool,
    /// Raw value of the `maxspeed` tag, unparsed. Use [`parse_max_speed`] with a
    /// named-value map to resolve this to km/h.
    pub raw_max_speed: Option<&'s str>,
    pub raw_max_speed_forward: Option<&'s str>,
    pub raw_max_speed_backward: Option<&'s str>,
    pub tunnel: bool,
    pub bridge: bool,
    pub ferry: bool,
    pub raw_max_height: Option<&'s str>,
    pub raw_max_width: Option<&'s str>,
    pub raw_max_weight: Option<&'s str>,
}

/*
impl<'s, T> Conditional<'s,T> {
    pub fn normalize(&mut self) {
        let Self::Multi(v) = self else { return };
        // TODO: normalize
    }
}
*/

impl<'s> WayTags<'s> {
    pub fn set_tag(&mut self, k: &str, v: &'s str) -> bool {
        match k {
            "highway" => self.highway = FromTag::from_tag(v),
            "toll" => self.toll = FromTag::from_tag(v),
            "surface" => self.surface = FromTag::from_tag(v),
            "smoothness" => self.smoothness = FromTag::from_tag(v),
            "tracktype" => self.tracktype = FromTag::from_tag(v),
            "junction" => self.junction = FromTag::from_tag(v),
            "motorroad" => self.motorroad = FromTag::from_tag(v),
            "disused" => self.disused = FromTag::from_tag(v),
            "abandoned" => self.abandoned = FromTag::from_tag(v),
            "maxspeed" => self.raw_max_speed = Some(v),
            "maxspeed:forward" => self.raw_max_speed_forward = Some(v),
            "maxspeed:backward" => self.raw_max_speed_backward = Some(v),
            "tunnel" => self.tunnel = !matches!(v, "no" | "false"),
            "bridge" => self.bridge = !matches!(v, "no" | "false"),
            "route" => self.ferry = v == "ferry",
            "maxheight" => self.raw_max_height = Some(v),
            "maxwidth" => self.raw_max_width = Some(v),
            "maxweight" => self.raw_max_weight = Some(v),
            _ => {
                let mut k2 = k;
                if let Some(p) = k2.find(':') {
                    k2 = &k2[0..p];
                }
                match k2 {
                    "oneway" => self.oneway.set_tag(k, v),
                    "access" => {
                        if !k.split(':').any(|s| s == "lanes") {
                            self.access.set_tag(k, v)
                        }
                    }
                    _ if !matches!(Mode::from_tag(k2), Mode::default | Mode::unknown) => {
                        if !k.split(':').any(|s| s == "lanes") {
                            self.access.set_tag(k, v)
                        }
                    }
                    _ => return false,
                }
            }
        }
        true
    }

    /*
    // TODO
    pub fn normalize(&mut self) {
        if let Some(highway) = self.highway {
            if self.motorroad {
                Access::no.set_default(&mut self.foot);
                Access::no.set_default(&mut self.bycicle);
            }
            match highway {
                Highway::motorway | Highway::motorway_link => {
                    Access::yes.set_default(&mut self.motorcar);
                    Access::yes.set_default(&mut self.motorcycle);
                    Access::yes.set_default(&mut self.hgv);
                    if Access::no.set_default(&mut self.access) {
                        if Access::yes.set_default(&mut self.motor_vehicle) {
                            Access::no.set_default(&mut self.moped);
                            Access::no.set_default(&mut self.mofa);
                        }
                        Access::no.set_default(&mut self.horse);
                        Access::no.set_default(&mut self.bycicle);
                        Access::no.set_default(&mut self.foot);
                    }
                    Surface::paved.set_default(&mut self.surface);
                },
                Highway::pedestrian => {
                    Access::yes.set_default(&mut self.foot);
                    if Access::no.set_default(&mut self.access) {
                        Access::no.set_default(&mut self.motor_vehicle);
                        Access::no.set_default(&mut self.bycicle);
                        Access::no.set_default(&mut self.horse);
                    }
                }
                Highway::busway => {
                    if Access::no.set_default(&mut self.access) {
                        //Access::designated.set_default(&mut self.bus);
                    }
                },
                Highway::footway => {
                    Access::designated.set_default(&mut self.foot);
                    if Access::no.set_default(&mut self.access) {
                        Access::no.set_default(&mut self.motor_vehicle);
                        Access::no.set_default(&mut self.bycicle);
                        Access::no.set_default(&mut self.horse);
                    }
                },
                Highway::bridleway => {
                    Access::designated.set_default(&mut self.horse);
                    if Access::no.set_default(&mut self.access) {
                        Access::no.set_default(&mut self.motor_vehicle);
                        Access::no.set_default(&mut self.bycicle);
                        Access::no.set_default(&mut self.foot);
                    }
                },
                Highway::cycleway => {
                    Access::designated.set_default(&mut self.bycicle);
                    if Access::no.set_default(&mut self.access) {
                        Access::no.set_default(&mut self.motor_vehicle);
                        Access::no.set_default(&mut self.horse);
                        Access::no.set_default(&mut self.foot);
                    }
                },
                Highway::path => {
                    Access::yes.set_default(&mut self.foot);
                    Access::yes.set_default(&mut self.bycicle);
                    Access::yes.set_default(&mut self.horse);
                    if Access::no.set_default(&mut self.access) {
                        Access::no.set_default(&mut self.motor_vehicle);
                    }
                },
                _ => {
                    Access::yes.set_default(&mut self.access);
                }
            }
        }
    }
    */
    pub fn is_excluded(&self) -> bool {
        (!self.ferry && self.highway.is_none_or(|h| h.is_excluded()))
            || self.access.is_excluded()
            || self.disused
            || self.abandoned
    }
}

/// Parse an OSM `maxspeed` tag value into km/h, capped at 255.
///
/// Lookup order:
/// 1. `named` map — handles any named value including country-coded ones
///    (`"DE:urban"`, `"GB:nsl_dual"`) and can override generics (`"urban"`, `"walk"`).
/// 2. Built-in fallbacks for generic names (for when the map has no entry).
/// 3. Imperial suffix (`"30 mph"`).
/// 4. Plain integer km/h.
///
/// Returns `None` for unrecognised named values, letting the caller fall back
/// to the profile's highway-class default.
pub fn parse_max_speed(v: &str, named: &std::collections::HashMap<String, u8>) -> Option<u8> {
    let v = v.trim();
    // Config map takes priority over everything — covers both "DE:urban" and "urban".
    if let Some(&kmh) = named.get(v) {
        return Some(kmh);
    }
    // Built-in fallbacks for generic names not present in the map.
    match v {
        "walk" | "walking" => return Some(7),
        "living_street" => return Some(10),
        "urban" => return Some(50),
        "rural" => return Some(90),
        "motorway" => return Some(130),
        _ => {}
    }
    if let Some(mph) = v.strip_suffix(" mph").or_else(|| v.strip_suffix("mph")) {
        let kmh = mph.trim().parse::<u16>().ok()? * 1609 / 1000;
        return Some(kmh.min(255) as u8);
    }
    let kmh = v.parse::<u16>().ok()?;
    Some(kmh.min(255) as u8)
}

#[derive(Default)]
pub struct NodeTags<'s> {
    pub barrier: Option<Barrier>,
    pub access: Conditional<'s, Access>,
    pub highway: Option<NodeHighway>,
    pub toll: Option<bool>,
}

impl<'s> NodeTags<'s> {
    pub fn set_tag(&mut self, k: &str, v: &'s str) -> bool {
        match k {
            "barrier" => self.barrier = FromTag::from_tag(v),
            "highway" => self.highway = FromTag::from_tag(v),
            "toll" => self.toll = FromTag::from_tag(v),
            _ => {
                let k2 = k.split(':').next().unwrap_or(k);
                match k2 {
                    "access" => self.access.set_tag(k, v),
                    _ if !matches!(Mode::from_tag(k2), Mode::default | Mode::unknown) => {
                        self.access.set_tag(k, v)
                    }
                    _ => return false,
                }
            }
        }
        true
    }

    pub fn has_routing_data(&self) -> bool {
        self.barrier.is_some()
            || !self.access.is_none()
            || self.highway.is_some()
            || self.toll.is_some()
    }
}

impl Conditional<'_, Access> {
    pub fn is_excluded(&self) -> bool {
        match self {
            Self::None => false,
            Self::Simple(a) => a.is_excluded(),
            // Only exclude a way when a general (no mode/direction/condition) access
            // entry is explicitly prohibited. Mode- or direction-specific restrictions
            // (e.g. `foot:left=no`) do not mean the way is globally inaccessible.
            Self::Multi(v) => {
                let mut has_general = false;
                for c in v {
                    if c.mode == Mode::default
                        && c.direction == Direction::both
                        && c.condition.is_none()
                    {
                        has_general = true;
                        if !c.value.is_excluded() {
                            return false;
                        }
                    }
                }
                has_general
            }
        }
    }
}
