#![feature(external_doc)] // `#[doc(include = ...)]`
#![doc(include = "./lib.md")]
mod codegen;
mod metadata;

pub use self::codegen::BuildScriptConfig;
