#[macro_use] extern crate log;
extern crate env_logger;
extern crate byteorder;
extern crate rustc_serialize;

use std::io::{Read, Cursor, Result as Result};
use std::str::FromStr;

use byteorder::{ReadBytesExt, LittleEndian};
use rustc_serialize::json::{Json, ToJson};

macro_rules! custom_debug {
    ($depth:expr, $fmt:expr, $($arg:tt)+) => {debug!(concat!("{}", $fmt), std::iter::repeat(" ").take($depth as usize*2).collect::<String>(), $($arg)+)};
    ($depth:expr, $fmt:expr) => {debug!(concat!("{}", $fmt), std::iter::repeat(" ").take($depth as usize*2).collect::<String>())}
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PropertyType {
    Bool,
    Byte,
    Int,
    Float,
    Str,
    LinearColor,
    Array,
    Struct,
    Object,
    // i have no idea, what i'm doing
    CharacterDNA,
    CharacterCustomization,

    Dunno(String),
}

impl FromStr for PropertyType {
    type Err = ();
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "BoolProperty" => PropertyType::Bool,
            "ByteProperty" => PropertyType::Byte,
            "IntProperty" => PropertyType::Int,
            "FloatProperty" => PropertyType::Float,
            "StrProperty" => PropertyType::Str,
            "LinearColor" => PropertyType::LinearColor,
            "ArrayProperty" => PropertyType::Array,
            "StructProperty" => PropertyType::Struct,
            "ObjectProperty" => PropertyType::Object,
            // I have no idea what I'm doing
            "CharacterDNA" => PropertyType::CharacterDNA,
            "CharacterCustomization" => PropertyType::CharacterCustomization,
            s => PropertyType::Dunno(s.to_string())
        })
    }
}

impl std::fmt::Display for PropertyType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, PartialEq)]
struct Value(String, ReturnType);

impl Value {
    fn new(name: String, value: ReturnType) -> Value {
        Value(name, value)
    }
}

#[derive(Debug, PartialEq)]
enum ReturnType {
    Bool(bool),
    Byte(u64),
    Int(i32),
    Float(f32),
    Str(String),
    LinearColor(Vec<u8>),
    Array(Vec<ReturnType>),
    Struct(Vec<Value>),
    Object(String),
    CharacterDNA((u64, u64)),
    CharacterCustomization((u64, u64)),
}

impl ToJson for ReturnType {
    fn to_json(&self) -> Json {
        match self {
            &ReturnType::Bool(b) => Json::Boolean(b),
            &ReturnType::Byte(b) => Json::U64(b),
            &ReturnType::Int(i) => Json::I64(i as i64),
            &ReturnType::Float(f) => Json::F64(f as f64),
            &ReturnType::Str(ref s) => Json::String(s.clone()),
            &ReturnType::LinearColor(ref v) => Json::Array(v.iter().cloned().map(|b| Json::U64(b as u64)).collect()),
            &ReturnType::Array(ref v) => Json::Array(v.iter().map(|e| e.to_json()).collect()),
            &ReturnType::Struct(ref v) => Json::Object(v.iter().map(|v| (v.0.clone(), v.1.to_json())).collect()),
            &ReturnType::Object(ref s) => Json::String(s.clone()),
            &ReturnType::CharacterDNA((a,b)) => Json::Array(vec![Json::U64(a), Json::U64(b)]),
            &ReturnType::CharacterCustomization((a,b)) => Json::Array(vec![Json::U64(a), Json::U64(b)]),
        }
    }
}

trait GVASRead {
    fn parse(&mut self) -> Result<ReturnType>;
    fn parse_internal(&mut self, depth: u8) -> Result<ReturnType>;
    fn parse_type(&mut self, t: PropertyType, read_len: bool, depth: u8) -> Result<ReturnType>;
    fn read_head(&mut self) -> Result<()>;
    fn read_string(&mut self) -> Result<String>;
    fn read_type(&mut self) -> Result<PropertyType>;
}

impl<R: AsRef<[u8]>> GVASRead for Cursor<R> {
    fn parse(&mut self) -> Result<ReturnType> {
        self.read_head().unwrap();
        debug!("{}", try!(self.read_string()));
        debug!("{}", try!(self.read_string()));
        let mut vec = Vec::new();
        loop {
            let name = try!(self.read_string());
            if name == "None" {
                break;
            }
            vec.push(Value::new(name, try!(self.parse_internal(0))));
        }
        Ok(ReturnType::Struct(vec))
    }

    fn parse_internal(&mut self, depth: u8) -> Result<ReturnType> {
        let t = try!(self.read_type());
        self.parse_type(t, true, depth)
    }

    fn parse_type(&mut self, t: PropertyType, read_len: bool, depth: u8) -> Result<ReturnType> {
        match t {
            PropertyType::Array => {
                let len = try!(self.read_u64::<LittleEndian>());
                let typ = try!(self.read_type());
                let elements = try!(self.read_u32::<LittleEndian>());
                custom_debug!(depth, "{}: {}, {}: {}", t, len, typ, elements);
                custom_debug!(depth, "[");
                let mut res = Vec::new();
                for _ in 0..elements {
                    res.push(try!(self.parse_type(typ.clone(), false, depth+1)));
                }
                custom_debug!(depth, "]");
                // Usually arrays are finished with `None`.
                // For some reason the outer-most array does not end with `None`.
                // Instead the next Struct-elements are continued.
                // Therefore if we get a non-`None` value, we reset the cursor.
                let pos = self.position();
                let none = try!(self.read_string());
                if none != "None" {
                    self.set_position(pos);
                }
                Ok(ReturnType::Array(res))
            },
            PropertyType::Struct => {
                // TODO: use Option for len
                let len;
                if read_len {
                    len = try!(self.read_u64::<LittleEndian>());
                    custom_debug!(depth, "{}: {}", t, len);
                } else {
                    len = 0;
                    custom_debug!(depth,"{}", t);
                }
                custom_debug!(depth, "{{");
                let start_pos = self.position();
                let mut res = Vec::new();
                loop {
                    // Structs usually have <name> <type> <value>.
                    // For some reason, <type> is not given for CharacterDNA and CharacterCustomization.
                    // For these types we directly interpret them as type.
                    // If there is <type> <value>, we will get a type and parse it,
                    // otherwise we get <name> <type> <value> and parse it accordingly.
                    // There is also the possibility to get "None", which leads to a continue.
                    let mut typ = try!(self.read_type());
                    let name;
                    if let PropertyType::Dunno(s) = typ {
                        if s == "None" {
                            debug!("NONE NONE NONE");
                            continue;
                        }
                        name = s;
                        typ = try!(self.read_type());
                    } else {
                        name = typ.to_string();
                    }
                    custom_debug!(depth, "name: {}", name);
                    let value = try!(self.parse_type(typ, true, depth+2));
                    let cond = (len == 0 || self.position() < start_pos + len) && name != "EntitlementsSeen";
                    res.push(Value::new(name, value));
                    if !cond {
                        break;
                    }
                }
                custom_debug!(depth, "}}");
                Ok(ReturnType::Struct(res))
            },
            PropertyType::Str => {
                let len = try!(self.read_u64::<LittleEndian>());
                let buf = self.take(len).bytes().map(|b| b.unwrap()).collect::<Vec<_>>();
                let s = try!(Cursor::new(buf).read_string());
                custom_debug!(depth, "{}: {:?}", t, s);
                Ok(ReturnType::Str(s))
            },
            PropertyType::Byte => {
                let byte = try!(self.read_u64::<LittleEndian>());
                custom_debug!(depth, "{}: {}", t, byte);
                Ok(ReturnType::Byte(byte))
            },
            PropertyType::Bool => {
                let len = try!(self.read_u64::<LittleEndian>());
                assert_eq!(len, 0);
                let b = try!(self.read_u8()) == 1;
                custom_debug!(depth, "{}: {:?}", t, b);
                Ok(ReturnType::Bool(b))
            },
            PropertyType::Float => {
                let len = try!(self.read_u64::<LittleEndian>());
                assert_eq!(len, 4);
                let float = try!(self.read_f32::<LittleEndian>());
                custom_debug!(depth, "{}: {:?}", t, float);
                Ok(ReturnType::Float(float))
            },
            PropertyType::LinearColor => {
                let len = try!(self.read_u64::<LittleEndian>());
                assert_eq!(len, 0);
                let buf = self.take(24).bytes().map(|b| b.unwrap()).collect::<Vec<_>>();
                custom_debug!(depth, "{}: {:?}", t, buf);
                Ok(ReturnType::LinearColor(buf))
            },
            PropertyType::Object => {
                if read_len {
                    try!(self.read_u64::<LittleEndian>());
                }
                let obj = try!(self.read_string());
                custom_debug!(depth, "{}: {}", t, obj);
                Ok(ReturnType::Object(obj))
            },
            // quick fixes
            PropertyType::Int => {
                if read_len {
                    try!(self.read_u64::<LittleEndian>());
                }
                let i = try!(self.read_i32::<LittleEndian>());
                custom_debug!(depth, "{}: {}", t, i);
                Ok(ReturnType::Int(i))
            },
            // I have no idea what I'm doing
            PropertyType::CharacterDNA => {
                let a = try!(self.read_u64::<LittleEndian>());
                let b = try!(self.read_u64::<LittleEndian>());
                custom_debug!(depth, "{}: {}, {}", t, a, b);
                Ok(ReturnType::CharacterDNA((a, b)))
            },
             PropertyType::CharacterCustomization => {
                let a = try!(self.read_u64::<LittleEndian>());
                let b = try!(self.read_u64::<LittleEndian>());
                custom_debug!(depth, "{}: {}, {}", t, a, b);
                Ok(ReturnType::CharacterCustomization((a, b)))
            },
            PropertyType::Dunno(s) => {
                custom_debug!(depth, "{}", s);
                Ok(ReturnType::Str(s))
            }
        }
    }

    fn read_head(&mut self) -> Result<()> {
        let buf = self.take(22).bytes().map(|b| b.unwrap()).collect::<Vec<_>>();
        debug!("{:?}", buf);
        let mut b2 = buf.clone();
        b2.resize(4, 0);
        assert_eq!("GVAS", &String::from_utf8(b2).unwrap());
        Ok(())
    }

    fn read_string(&mut self) -> Result<String> {
        let len = try!(self.read_u32::<LittleEndian>());
        let mut buf = self.take(len as u64).bytes().map(|b| b.unwrap()).collect::<Vec<_>>();
        assert_eq!(buf.pop().unwrap(), 0);
        Ok(String::from_utf8(buf).unwrap())
    }
    
    fn read_type(&mut self) -> Result<PropertyType> {
        Ok(PropertyType::from_str(&try!(self.read_string())).unwrap())
    }
}

fn main() {
    env_logger::init().unwrap();
    let mut buf = Vec::new();
    std::io::stdin().read_to_end(&mut buf).unwrap();
    let mut cur = Cursor::new(buf);
    let res = cur.parse().unwrap();
    info!("{:?}", res);
    let encoded = res.to_json().to_string();
    println!("{}", encoded);
}
