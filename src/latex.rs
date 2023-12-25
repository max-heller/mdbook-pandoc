use std::collections::BTreeSet;

#[derive(Default)]
pub struct Packages {
    needed: BTreeSet<Package>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Package {
    FontAwesome,
}

impl Packages {
    pub fn need(&mut self, package: Package) {
        self.needed.insert(package);
    }

    pub fn needed(&self) -> impl Iterator<Item = Package> + '_ {
        self.needed.iter().cloned()
    }
}

impl Package {
    pub fn name(&self) -> &str {
        match self {
            Self::FontAwesome => "fontawesome",
        }
    }
}
