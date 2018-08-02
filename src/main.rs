extern crate env_logger;
extern crate serde;
extern crate serde_gvas;
#[macro_use]
extern crate serde_derive;

use serde::{Deserialize, de::value::MapAccessDeserializer};
use std::io::Read;
use std::fs::{self, File, FileType};
use std::path::PathBuf;

#[derive(Deserialize, Debug)]
struct Foo {
    Kills: i32,
    Deaths: i32,
    Wins: i32,
    Losses: i32,
}

fn main() {
    env_logger::init();

    let path = get_path("Victory");
    for sav in std::fs::read_dir(path).unwrap() {
        let sav = sav.unwrap();
        if !sav.file_type().unwrap().is_file() {
            continue;
        }
        let extension = sav.path().extension();
        if extension.is_none() || extension.unwrap() != "sav" {
            continue;
        }

        let file = File::open(sav.path());
        let mut head = [0u8; 47];
        file.read_exact(&mut head).unwrap();
        if head[0..4] != b"GVAS" || head[26..31] != b"++RedHarvest+Staging" {
            continue;
        }
        let (mut de, _) = serde_gvas::Deserializer::new(file).unwrap();
        let mut de = MapAccessDeserializer::new(serde_gvas::MapDeserializer::new(&mut de));
        let foo = Foo::deserialize(de);
        println!("{:?}", foo);
    }
}

fn get_path(game: &'static str) -> PathBuf {
    let mut path = std::env::home_dir().unwrap();
     if cfg!(windows) {
        path.push("Appdata"); path.push("Local");
    } else {
        path.push(".config"); path.push("Epic");
    };
    path.push("Victory"); path.push("Saved");
    path.push("SaveGames");
    path
}