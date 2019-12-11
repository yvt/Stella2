use serde::{Deserialize, Serialize};
use std::fmt;
use try_match::try_match;
use uuid::Uuid;

pub mod visit_mut;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    /// Specifies which element of `crates` is the main crate. Other elements
    /// are its dependencies.
    pub main_crate_i: usize,
    pub crates: Vec<Crate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Crate {
    /// Should be used only for creating diagnostic messages and generating
    /// implementation code.
    pub name: String,
    pub uuid: Uuid,
    pub comps: Vec<CompDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Visibility {
    Private,
    /// This variant is meaningful only for the current crate. Can be replaced
    /// with `Private` when exporting the metadata to a file in accordance with
    /// the one-crate-one-file rule.
    Restricted(Path),
    Public,
}

/// The absolute path to an item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Path {
    /// Index into `Repo::crates`
    pub crate_i: usize,
    pub idents: Vec<Ident>,
}

pub type Ident = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompDef {
    pub flags: CompFlags,
    pub vis: Visibility,
    /// The path of the component's type. Note that a component can have
    /// multiple aliases.
    pub paths: Vec<Path>,
    pub items: Vec<CompItemDef>,
}

bitflags::bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct CompFlags: u8 {
        /// Do not generate implementation code.
        const PROTOTYPE_ONLY = 1;

        /// The component represents a widget.
        const WIDGET = 1 << 1;

        /// The component uses the simple builder API.
        /// Requires `PROTOTYPE_ONLY`.
        const SIMPLE_BUILDER = 1 << 2;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompItemDef {
    Field(FieldDef),
    Event(EventDef),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub field_ty: FieldType,
    pub flags: FieldFlags,
    pub ident: Ident,
    pub accessors: FieldAccessors,
    /// `Some(_)` if the field type refers to a component. `None` otherwise.
    pub ty: Option<CompRef>,
}

bitflags::bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct FieldFlags: u8 {
        const INJECT = 1;

        /// Only valid in `metadata`. Only relevant for `FieldType::{Const, Prop}`.
        const OPTIONAL = 1 << 1;
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum FieldType {
    Prop,
    Const,
    Wire,
}

/// A reference to a `CompDef` in the same `Repo`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CompRef {
    pub crate_i: usize,
    pub comp_i: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldAccessors {
    /// Valid only for `prop`
    pub set: Option<FieldSetter>,
    /// Valid for all field types
    pub get: Option<FieldGetter>,
    /// Valid only for `prop` and `wire`
    pub watch: Option<FieldWatcher>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSetter {
    pub vis: Visibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldGetter {
    pub vis: Visibility,
    pub mode: FieldGetMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FieldGetMode {
    /// The getter returns `impl Deref<Target = T>`.
    Borrow,
    /// The getter returns `T`.
    Clone,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldWatcher {
    pub vis: Visibility,
    /// Refers to an event in the same component where the field is defined.
    pub event: Ident,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDef {
    pub vis: Visibility,
    pub ident: Ident,
    pub inputs: Vec<Ident>,
}

// Printing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct PathDisplay<'a> {
    path: PathRef<'a>,
    repo: &'a Repo,
}

impl Path {
    pub fn display<'a>(&'a self, repo: &'a Repo) -> PathDisplay<'a> {
        self.as_ref().display(repo)
    }
}

impl<'a> PathRef<'a> {
    pub fn display(&self, repo: &'a Repo) -> PathDisplay<'a> {
        PathDisplay { path: *self, repo }
    }
}

impl fmt::Display for PathDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.crate_i == self.repo.main_crate_i {
            write!(f, "crate")?;
        } else {
            write!(f, "::{}", self.repo.crates[self.path.crate_i].name)?;
        }

        for ident in self.path.idents.iter() {
            write!(f, "::{}", ident)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VisibilityDisplay<'a> {
    vis: VisibilityRef<'a>,
    repo: &'a Repo,
}

impl Visibility {
    #[allow(dead_code)]
    pub fn display<'a>(&'a self, repo: &'a Repo) -> VisibilityDisplay<'a> {
        self.as_ref().display(repo)
    }
}

impl<'a> VisibilityRef<'a> {
    pub fn display(&self, repo: &'a Repo) -> VisibilityDisplay<'a> {
        VisibilityDisplay { vis: *self, repo }
    }
}

impl fmt::Display for VisibilityDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.vis {
            VisibilityRef::Private => Ok(()),
            VisibilityRef::Restricted(p) => write!(f, "pub (in {})", p.display(self.repo)),
            VisibilityRef::Public => write!(f, "pub"),
        }
    }
}

// Metadata Manipulation
// ---------------------------------------------------------------------------

impl Repo {
    /// Find a `CompDef` by `CompRef`.
    pub fn comp_by_ref(&self, r: &CompRef) -> &CompDef {
        &self.crates[r.crate_i].comps[r.comp_i]
    }
}

/// The borrowed version of `Visiblity`.
#[derive(Debug, Clone, Copy)]
pub enum VisibilityRef<'a> {
    Private,
    Restricted(PathRef<'a>),
    Public,
}

impl Visibility {
    pub fn as_ref(&self) -> VisibilityRef<'_> {
        match self {
            Visibility::Private => VisibilityRef::Private,
            Visibility::Restricted(p) => VisibilityRef::Restricted(p.as_ref()),
            Visibility::Public => VisibilityRef::Public,
        }
    }
}

impl VisibilityRef<'_> {
    pub fn strictest(self, other: Self) -> Self {
        match (self, other) {
            (Self::Private, _) => Self::Private,
            (_, Self::Private) => Self::Private,
            (Self::Restricted(p), Self::Public) => Self::Restricted(p),
            (Self::Public, Self::Restricted(p)) => Self::Restricted(p),
            (Self::Restricted(p1), Self::Restricted(p2)) => {
                if let Some(p) = p1.lowest_common_ancestor(&p2) {
                    Self::Restricted(p)
                } else {
                    Self::Private
                }
            }
            (Self::Public, Self::Public) => Self::Public,
        }
    }
}

/// The borrowed version of `Path`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathRef<'a> {
    /// Index into `Repo::crates`
    crate_i: usize,
    idents: &'a [Ident],
}

impl Path {
    pub fn as_ref(&self) -> PathRef<'_> {
        PathRef {
            crate_i: self.crate_i,
            idents: &self.idents[..],
        }
    }
}

impl PathRef<'_> {
    pub fn to_owned(&self) -> Path {
        Path {
            crate_i: self.crate_i,
            idents: self.idents.to_owned(),
        }
    }

    pub fn parent(&self) -> Option<Self> {
        if self.idents.is_empty() {
            None
        } else {
            Some(Self {
                crate_i: self.crate_i,
                idents: &self.idents[..self.idents.len() - 1],
            })
        }
    }

    pub fn starts_with(&self, other: &PathRef<'_>) -> bool {
        self.crate_i == other.crate_i && self.idents.starts_with(other.idents)
    }

    pub fn lowest_common_ancestor(&self, other: &Self) -> Option<Self> {
        if self.crate_i == other.crate_i {
            let len = self
                .idents
                .iter()
                .zip(other.idents.iter())
                .take_while(|(a, b)| a == b)
                .count();
            Some(Self {
                crate_i: self.crate_i,
                idents: &self.idents[..len],
            })
        } else {
            None
        }
    }
}

impl CompItemDef {
    pub fn field(&self) -> Option<&FieldDef> {
        try_match!(Self::Field(field) = self).ok()
    }

    pub fn event(&self) -> Option<&EventDef> {
        try_match!(Self::Event(event) = self).ok()
    }

    pub fn ident(&self) -> &Ident {
        match self {
            CompItemDef::Field(field) => &field.ident,
            CompItemDef::Event(event) => &event.ident,
        }
    }
}

impl CompDef {
    pub fn name(&self) -> &Ident {
        self.paths[0].idents.last().unwrap()
    }

    /// Calculate the maximum possibile visibility of the component's builder
    /// type can have. Having a visibility beyond this is pointless on account
    /// of `const` fields that can't be initialized.
    pub fn builder_vis(&self) -> VisibilityRef<'_> {
        self.items
            .iter()
            .filter_map(|item| match item {
                CompItemDef::Field(FieldDef {
                    field_ty: FieldType::Wire,
                    ..
                }) => None,
                // Non-optional `const` and `prop` fields
                CompItemDef::Field(FieldDef {
                    flags,
                    accessors:
                        FieldAccessors {
                            set: Some(FieldSetter { vis }),
                            ..
                        },
                    ..
                }) if !flags.contains(FieldFlags::OPTIONAL) => Some(vis.as_ref()),
                _ => None,
            })
            .fold(self.vis.as_ref(), VisibilityRef::strictest)
    }

    pub fn find_item_by_ident(&self, ident: &str) -> Option<(usize, &CompItemDef)> {
        self.items
            .iter()
            .enumerate()
            .find(|(_, item)| item.ident() == ident)
    }
}
