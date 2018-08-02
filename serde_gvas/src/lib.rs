#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde;
extern crate byteorder;
#[macro_use]
extern crate log;
extern crate encoding_rs;
extern crate void;

mod error;
mod de;
mod ser;

pub use error::{Error, Result};
pub use de::{Deserializer, MapDeserializer};
pub use ser::Serializer;

// TODO: from_bytes, from_reader
// TODO: to_XXX