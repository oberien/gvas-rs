use std::io::{Read, Cursor};
use std::str::FromStr;

use byteorder::{ReadBytesExt, LE};
use encoding_rs::{Encoding, WINDOWS_1252, UTF_16LE};
use serde::{self, de::{Visitor, SeqAccess, MapAccess, DeserializeSeed, IntoDeserializer}};
use void::Void;

use error::{Result, Error, ErrorKind};

//pub fn from_bytes(bytes: &[u8]) -> Result<Deserializer<Cursor<&[u8]>>> {
//    Deserializer::new(Cursor::new(bytes))
//}
//
//pub fn from_reader<R: Read>(r: R) -> Result<Deserializer<R>> {
//    Deserializer::new(r)
//}

pub struct Deserializer<R: Read> {
    r: R,
    read: usize,
}

// Format starts with header, followed by struct-name, followed by Struct.
// We ignore the header and let the user parse that before he hands over the reader to us.
//
// Struct: Map<String, Property>
// Property: (TypeString, lenI32, unknownI32, Value)
// Value: Struct | Array | Object | Primitive | External
// Primitive: Bool | Int | QWord | Float | Str | Byte (actually enum)
// Byte (actually enum): (EnumNameString, VariantString)
// External (???): LinearColor | CharacterDNA | CharacterCustomization | NameProperty | ...

impl<R: Read> Deserializer<R> {
    /// Create a new Deserializer and return the name of the serialized struct
    ///
    /// This method assumes that the header is already parsed and the reader starts
    /// at the first value (i.e. it's name).
    pub fn new(mut r: R) -> Result<(Deserializer<R>, String)> {
        let (name, len) = parse_string(&mut r, 0)?;
        Ok((Deserializer {
            r,
            read: len,
        }, name))
    }

    fn parse_type(&mut self) -> Result<(PropertyType, u32)> {
        trace!("parse_type");
        let s = self.parse_str(None)?;
        let typ = PropertyType::from_str(&s).unwrap();
        let len = self.r.read_u32::<LE>()?;
        let _unknown = self.r.read_u32::<LE>()?;
        self.read += 8;
        trace!("{:?}, {}, {}", typ, len, _unknown);
        Ok((typ, len))
    }

    fn parse_type_in_array(&mut self) -> Result<PropertyType> {
        trace!("parse_type_in_array");
        let s = self.parse_str(None)?;
        let typ = PropertyType::from_str(&s).unwrap();
        trace!("{:?}", typ);
        Ok(typ)
    }

    fn parse_bool(&mut self, len: Option<u32>) -> Result<bool> {
        trace!("parse_bool");
        // len for bool is broken, ignore it
        self.read += 1;
        let b = self.r.read_u8()? == 1;
        trace!("{}", b);
        Ok(b)
    }

    fn parse_int(&mut self, len: Option<u32>) -> Result<i32> {
        trace!("parse_int");
        if len.is_some() && len.unwrap() != 4 {
            return Err(Error::new(ErrorKind::InvalidIntLength(len.unwrap()), self.read));
        }
        let i = self.r.read_i32::<LE>()?;
        self.read += 4;
        trace!("{}", i);
        Ok(i)
    }

    fn parse_qword(&mut self, len: Option<u32>) -> Result<i64> {
        trace!("parse_qword");
        if len.is_some() && len.unwrap() != 8 {
            return Err(Error::new(ErrorKind::InvalidQwordLength(len.unwrap()), self.read));
        }
        let i = self.r.read_i64::<LE>()?;
        self.read += 8;
        trace!("{}", i);
        Ok(i)
    }

    fn parse_float(&mut self, len: Option<u32>) -> Result<f32> {
        trace!("parse_float");
        if len.is_some() && len.unwrap() != 4 {
            return Err(Error::new(ErrorKind::InvalidFloatLength(len.unwrap()), self.read));
        }
        let f = self.r.read_f32::<LE>()?;
        self.read += 4;
        trace!("{}", f);
        Ok(f)
    }

    fn parse_str(&mut self, len: Option<u32>) -> Result<String> {
        trace!("parse_str");
        let (s, slen) = parse_string(&mut self.r, self.read)?;
        if len.is_some() && len.unwrap() as usize != slen {
            return Err(Error::new(ErrorKind::InvalidStringLength(len.unwrap()), self.read));
        }
        self.read += slen;
        trace!("{:?}", s);
        Ok(s)
    }

    /// Parses a ByteProperty, which is an enum variant
    ///
    /// Returns a tuple containing the enum-name and the enum varinant
    fn parse_byte(&mut self, len: Option<u32>) -> Result<(String, String)> {
        trace!("parse_byte");
        let (name, nlen) = parse_string(&mut self.r, self.read)?;
        let (variant, vlen) = parse_string(&mut self.r, self.read)?;
        if len.is_some() && len.unwrap() as usize != nlen + vlen {
            return Err(Error::new(ErrorKind::InvalidStringLength(len.unwrap()), self.read));
        }
        self.read += nlen + vlen;
        trace!("{:?}, {:?}", name, variant);
        Ok((name, variant))
    }

    fn parse_object(&mut self, len: Option<u32>) -> Result<String> {
        trace!("parse_object");
        let obj = self.parse_str(len)?;
        trace!("{:?}", obj);
        Ok(obj)
    }

    fn visit_type<'de, V: Visitor<'de>>(&mut self, v: V, typ: PropertyType, len: Option<u32>) -> Result<V::Value> {
        trace!("visit_type: {:?}", typ);
        match typ {
            PropertyType::Bool => v.visit_bool(self.parse_bool(len)?),
            PropertyType::Int => v.visit_i32(self.parse_int(len)?),
            PropertyType::Qword => v.visit_i64(self.parse_qword(len)?),
            PropertyType::Float => v.visit_f32(self.parse_float(len)?),
            PropertyType::Str => v.visit_string(self.parse_str(len)?),
            PropertyType::Object => v.visit_string(self.parse_str(len)?),
            PropertyType::Byte => v.visit_enum(self.parse_byte(len)?.1.into_deserializer()),
            PropertyType::Array => v.visit_seq(ArrayDeserializer { de: self }),
            PropertyType::Struct => unimplemented!(),
            PropertyType::Unknown(s) => v.visit_map(MapDeserializer::new(self)),
        }
    }
}

impl<'a, 'de, R: Read> serde::Deserializer<'de> for &'a mut Deserializer<R> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        trace!("deserialize_any");
        // TODO: in_array
        let (typ, len) = self.parse_type()?;
        let len = Some(len);

        self.visit_type(v, typ, len)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

struct ArrayDeserializer<'a, T: Read + 'a> {
    de: &'a mut Deserializer<T>,
}

impl<'de, 'a, T: Read> SeqAccess<'de> for ArrayDeserializer<'a, T> {
    type Error = Error;

    fn next_element_seed<S: DeserializeSeed<'de>>(&mut self, seed: S) -> Result<Option<S::Value>> {
        let typ = self.de.parse_type_in_array()?;
        unimplemented!()
    }
}

pub struct MapDeserializer<'a, T: Read + 'a> {
    de: &'a mut Deserializer<T>,
}

impl<'a, T: Read + 'a> MapDeserializer<'a, T> {
    pub fn new(de: &'a mut Deserializer<T>) -> MapDeserializer<'a, T> {
        MapDeserializer { de }
    }
}

impl<'a, 'de, T: Read + 'a> MapAccess<'de> for MapDeserializer<'a, T> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        trace!("next_key_seed");
        let key = self.de.parse_str(None)?;
        if key == "None" {
            return Ok(None);
        }
        seed.deserialize(key.into_deserializer()).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        trace!("next_value_seed");
        seed.deserialize(&mut *self.de)
    }
}

fn parse_string<R: Read>(mut r: R, read: usize) -> Result<(String, usize)> {
    let len = r.read_i32::<LE>()?;
    // positive len: WINDOWS_1252, negative len: UTF_16LE
    if len == 0 {
        Err(Error::new(ErrorKind::InvalidStringLength(0), read))
    } else if len > 0 {
        let mut buf = vec![0u8; len as usize];
        r.read_exact(&mut buf)?;
        let null_byte = buf.pop().unwrap();
        if null_byte != 0 {
            buf.push(null_byte);
            return Err(Error::new(ErrorKind::StringNotZeroTerminated(buf), read));
        }
        let s = WINDOWS_1252.decode_without_bom_handling(&buf).0.into_owned();
        Ok((s, buf.len() + 4))
    } else {
        let mut buf = vec![0u8; len.abs() as usize * 2];
        r.read_exact(&mut buf)?;
        let last = buf.pop().unwrap();
        let second_last = buf.pop().unwrap();
        if last != 0 && second_last != 0 {
            buf.push(last);
            buf.push(second_last);
            return Err(Error::new(ErrorKind::StringNotZeroTerminated(buf), read));
        }
        let s = UTF_16LE.decode_without_bom_handling(&buf).0.into_owned();
        Ok((s, buf.len() + 4))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PropertyType {
    Bool,
    Int,
    Qword,
    Float,
    Str,
    Object,
    Byte,
    Array,
    Struct,
    // i have no idea, what i'm doing
//    LinearColor,
//    CharacterDNA,
//    CharacterCustomization,
    Unknown(String),
}

impl FromStr for PropertyType {
    type Err = Void;
    fn from_str(s: &str) -> ::std::result::Result<Self, Void> {
        Ok(match s {
            "BoolProperty" => PropertyType::Bool,
            "IntProperty" => PropertyType::Int,
            "QWordProperty" => PropertyType::Qword,
            "FloatProperty" => PropertyType::Float,
            "StrProperty" => PropertyType::Str,
            "ObjectProperty" => PropertyType::Object,
            "ByteProperty" => PropertyType::Byte,
            "ArrayProperty" => PropertyType::Array,
            "StructProperty" => PropertyType::Struct,
            // I have no idea what I'm doing
//            "LinearColor" => PropertyType::LinearColor,
//            "CharacterDNA" => PropertyType::CharacterDNA,
//            "CharacterCustomization" => PropertyType::CharacterCustomization,
            s => PropertyType::Unknown(s.to_string())
        })
    }
}
