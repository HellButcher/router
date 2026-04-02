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
    fn is_excluded(self) -> bool {
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
        !matches!(self, Self::None)
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
    pub motorroad: bool,
    pub disused: bool,
    pub abandoned: bool,
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
            "motorroad" => self.motorroad = FromTag::from_tag(v),
            "disused" => self.disused = FromTag::from_tag(v),
            "abandoned" => self.abandoned = FromTag::from_tag(v),
            _ => {
                let mut k2 = k;
                if let Some(p) = k2.find(':') {
                    k2 = &k2[0..p];
                }
                match k2 {
                    "oneway" => self.oneway.set_tag(k, v),
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
        self.highway.is_none_or(|h| h.is_excluded()) || self.access.is_excluded()
    }
}

impl Conditional<'_, Access> {
    pub fn is_excluded(&self) -> bool {
        match self {
            Self::None => false,
            Self::Simple(a) => a.is_excluded(),
            Self::Multi(v) => v
                .iter()
                .all(|c| c.mode == Mode::unknown || c.value.is_excluded()),
        }
    }
}
