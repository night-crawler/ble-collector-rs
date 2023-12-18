use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

use btleplug::api::BDAddr;

pub(crate) struct ConnectedPeripherals {
    pub(crate) connection_map: BTreeMap<BDAddr, BTreeSet<&'static str>>,
}

impl ConnectedPeripherals {
    pub(crate) fn new<P, S, C>(poll: P, subscribe: S, by_characteristic: C) -> Self
    where
        P: IntoIterator<Item = BDAddr>,
        S: IntoIterator<Item = BDAddr>,
        C: IntoIterator<Item = BDAddr>,
    {
        let mut connection_map: BTreeMap<BDAddr, BTreeSet<&str>> = BTreeMap::new();
        for addr in poll {
            connection_map.entry(addr).or_default().insert("P");
        }
        for addr in subscribe {
            connection_map.entry(addr).or_default().insert("S");
        }
        for addr in by_characteristic {
            connection_map.entry(addr).or_default().insert("C");
        }
        Self { connection_map }
    }
    pub(crate) fn get_all(&self) -> BTreeSet<BDAddr> {
        self.connection_map.keys().cloned().collect()
    }
}

impl Display for ConnectedPeripherals {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut iter = self.connection_map.iter().peekable();
        write!(f, "{{")?;
        while let Some((addr, connection_types)) = iter.next() {
            write!(f, "{addr}: ")?;

            for connection_type in connection_types {
                write!(f, "{connection_type}")?;
            }

            if iter.peek().is_some() {
                write!(f, ", ")?;
            }
        }

        write!(f, "}}")
    }
}
