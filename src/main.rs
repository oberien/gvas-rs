extern crate byteorder;

use std::fs::File;
use std::io::{Read, Cursor, Result as Result};
use std::str::FromStr;

use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};

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

trait GVASRead : Read {
    fn parse(&mut self) -> Result<()>;
    fn parse_internal(&mut self, depth: u8) -> Result<()>;
    fn parse_type(&mut self, t: PropertyType, read_len: bool, depth: u8) -> Result<()>;
    fn read_head(&mut self) -> Result<()>;
    fn read_string(&mut self) -> Result<String>;
    fn read_type(&mut self) -> Result<PropertyType>;
}

impl<R: AsRef<[u8]>> GVASRead for Cursor<R> {
    fn parse(&mut self) -> Result<()> {
        self.read_head().unwrap();
        println!("{}", try!(self.read_string()));
        println!("{}", try!(self.read_string()));
        let s = try!(self.read_string());
        if s == "CurrentCharacterSlot" {
            println!("{}", s);
            try!(self.parse_internal(0));
            println!("{}", try!(self.read_string()));
        } else {
            println!("CurrentCharacterSlot");
            println!("Int: 0");
            println!("{}", s);
        }
        try!(self.parse_internal(0));
        self.parse_internal(0)
    }

    fn parse_internal(&mut self, depth: u8) -> Result<()> {
        let t = try!(self.read_type());
        self.parse_type(t, true, depth)
    }

    fn parse_type(&mut self, t: PropertyType, read_len: bool, depth: u8) -> Result<()> {
        print!("{}", std::iter::repeat(" ").take((depth*2) as usize).collect::<String>());
        match t {
            PropertyType::Array => {
                let len = try!(self.read_u64::<LittleEndian>());
                let typ = try!(self.read_type());
                let elements = try!(self.read_u32::<LittleEndian>());
                println!("{}: {}, {}: {}", t, len, typ, elements);
                println!("{}[", std::iter::repeat(" ").take(depth as usize*2+2).collect::<String>());
                for _ in 0..elements {
                    try!(self.parse_type(typ.clone(), false, depth+2));
                }
                assert_eq!(try!(self.read_string()), "None");
                println!("{}]", std::iter::repeat(" ").take(depth as usize*2+2).collect::<String>());
                Ok(())
            },
            PropertyType::Struct => {
                let len;
                if read_len {
                    len = try!(self.read_u64::<LittleEndian>());
                    println!("{}: {}", t, len);
                } else {
                    len = 0;
                    println!("{}", t);
                }
                let start_pos = self.position();
                while {
                    // i have no idea what i'm doing
                    let mut typ = try!(self.read_type());
                    if let PropertyType::Dunno(s) = typ {
                        let name = s;
                        println!("{}name: {}", std::iter::repeat(" ").take(depth as usize * 2 + 2).collect::<String>(), name);
                        typ = try!(self.read_type());
                    }
                    self.parse_type(typ, true, depth+2).is_ok() && (len == 0 || self.position() > start_pos + len)
                } {}
                Ok(())
            },
            PropertyType::Str => {
                let len = try!(self.read_u64::<LittleEndian>());
                let buf = self.take(len).bytes().map(|b| b.unwrap()).collect::<Vec<_>>();
                println!("{}: {:?}", t, try!(Cursor::new(buf).read_string()));
                Ok(())
            },
            PropertyType::Byte => {
                let byte = try!(self.read_u64::<LittleEndian>());
                println!("{}: {}", t, byte);
                Ok(())
            },
            PropertyType::Bool => {
                let len = try!(self.read_u64::<LittleEndian>());
                assert_eq!(len, 0);
                let b = try!(self.read_u8()) == 1;
                println!("{}: {:?}", t, b);
                Ok(())
            },
            PropertyType::Float => {
                let len = try!(self.read_u64::<LittleEndian>());
                assert_eq!(len, 4);
                let float = try!(self.read_f32::<LittleEndian>());
                println!("{}: {:?}", t, float);
                Ok(())
            },
            PropertyType::LinearColor => {
                let len = try!(self.read_u64::<LittleEndian>());
                assert_eq!(len, 0);
                let buf = self.take(24).bytes().map(|b| b.unwrap()).collect::<Vec<_>>();
                println!("{}: {:?}", t, buf);
                Ok(())
            },
            PropertyType::Object => {
                if read_len {
                    let len = try!(self.read_u64::<LittleEndian>());
                }
                println!("{}: {}", t, try!(self.read_string()));
                Ok(())
            },
            // quick fixes
            PropertyType::Int => {
                if read_len {
                    let len = try!(self.read_u64::<LittleEndian>());
                }
                println!("{}: {}", t, try!(self.read_u32::<LittleEndian>()));
                Ok(())
            },
            // I have no idea what I'm doing
            PropertyType::CharacterDNA | PropertyType::CharacterCustomization => {
                println!("{}: {}, {}", t, try!(self.read_u64::<LittleEndian>()), try!(self.read_u64::<LittleEndian>()));
                Ok(())
            },
            PropertyType::Dunno(s) => {
                println!("{}", s);
                Ok(())
            }
        }
    }

    fn read_head(&mut self) -> Result<()> {
        let mut buf = Vec::new();
        buf.resize(22, 0);
        try!(self.read_exact(&mut buf));
        println!("{:?}", buf);
        let mut b2 = buf.clone();
        b2.resize(4, 0);
        assert_eq!("GVAS", &String::from_utf8(b2).unwrap());
        Ok(())
    }

    fn read_string(&mut self) -> Result<String> {
        let len = try!(self.read_u32::<LittleEndian>());
        let mut buf = Vec::new();
        buf.resize(len as usize, 0);
        try!(self.read_exact(&mut buf));
        //println!("{}: {:?}", len, buf);
        assert_eq!(buf.pop().unwrap(), 0);
        Ok(String::from_utf8(buf).unwrap())
    }
    
    fn read_type(&mut self) -> Result<PropertyType> {
        Ok(PropertyType::from_str(&try!(self.read_string())).unwrap())
    }
}

fn main() {
    //let mut f = File::open("files/ChracterSlotSave.9.sav").unwrap();
    let mut f = File::open("/home/morpheus/.config/Epic/Victory/Saved/SaveGames/ChracterSlotSave.9.sav").unwrap();
    //let mut f = File::open("files/flai.sav").unwrap();
    let mut buf = Vec::new();
    f.read_to_end(&mut buf);
    let mut cur = Cursor::new(buf);
    cur.parse().unwrap();
}
