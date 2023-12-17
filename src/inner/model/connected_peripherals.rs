use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};

use btleplug::api::BDAddr;

pub(crate) struct ConnectedPeripherals {
    pub(crate) poll: BTreeSet<BDAddr>,
    pub(crate) subscribe: BTreeSet<BDAddr>,
    pub(crate) by_characteristic: BTreeSet<BDAddr>,
}

impl ConnectedPeripherals {
    pub(crate) fn get_all(&self) -> BTreeSet<BDAddr> {
        self.poll
            .iter()
            .cloned()
            .chain(self.subscribe.iter().cloned())
            .chain(self.by_characteristic.iter().cloned())
            .collect()
    }
}

impl Display for ConnectedPeripherals {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "poll: {:?}, subscribe: {:?}, by characteristic{:?}",
            self.poll, self.subscribe, self.by_characteristic
        )
    }
}
