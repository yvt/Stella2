use serde::{Deserialize, Serialize};

pub mod visit_mut;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crate {
    pub comps: Vec<CompDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Visibility {
    Private,
    /// This variant is used only for the current crate. Replaced with
    /// `Private` when exporting the metadata to a file in accordance with
    /// the one-crate-one-file rule.
    Restricted(Path),
    Public,
}

/// The absolute path to an item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Path {
    pub root: PathRoot,
    pub idents: Vec<Ident>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PathRoot {
    Crate,
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
}

bitflags::bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct FieldFlags: u8 {
        const INJECT = 1;
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum FieldType {
    Prop,
    Const,
    Wire,
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
