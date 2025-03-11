use std::collections::HashMap;

/// Bencoding
#[derive(PartialEq, Debug, Clone)]
enum BencodeValue<'a> {
    String(BString<'a>),
    Integer(BInteger),
    List(BList<'a>),
    Dictionary(BDictionary<'a>),
}

/// Strings are length-prefixed base ten followed by a colon and the string.
/// For example 4:spam corresponds to 'spam'.
#[derive(Hash, Eq, PartialEq, Debug, Clone)]
struct BString<'a> {
    content: &'a [u8],
}

/// Integers are represented by an 'i' followed by the number in base 10 followed by an 'e'.
/// For example i3e corresponds to 3 and i-3e corresponds to -3.
/// Integers have no size limitation. i-0e is invalid.
/// All encodings with a leading zero, such as i03e, are invalid,
/// other than i0e, which of course corresponds to 0.
#[derive(PartialEq, Debug, Clone)]
struct BInteger {
    value: i64,
}

///Lists are encoded as an 'l' followed by their elements (also bencoded) followed by an 'e'. For example l4:spam4:eggse corresponds to ['spam', 'eggs'].
#[derive(PartialEq, Debug, Clone)]
struct BList<'a> {
    items: Vec<BencodeValue<'a>>,
}

///Dictionaries are encoded as a 'd' followed by a list of alternating keys
///and their corresponding values followed by an 'e'.
///For example, d3:cow3:moo4:spam4:eggse corresponds to {'cow': 'moo', 'spam': 'eggs'}
///and d4:spaml1:a1:bee corresponds to {'spam': ['a', 'b']}.
///Keys must be strings and appear in sorted order (sorted as raw strings, not alphanumerics).
#[derive(PartialEq, Debug, Clone)]
struct BDictionary<'a> {
    dict: HashMap<BString<'a>, BencodeValue<'a>>,
}

/// Metainfo files (also known as .torrent files)
#[derive(Debug, PartialEq)]
struct Torrent {
    /// The URL of the tracker.
    announce: String,
    /// The metadata
    info: Info,
}

#[derive(Debug, PartialEq)]
struct Info {
    ///In the single file case, the name key is the name of a file,
    ///in the muliple file case, it's the name of a directory.
    name: String,

    /// piece length maps to the number of bytes in each piece the file is split into.
    /// For the purposes of transfer,
    /// files are split into fixed-size pieces which are all the same length
    /// except for possibly the last one which may be truncated.
    /// piece length is almost always a power of two,
    /// most commonly 2 18 = 256 K (BitTorrent prior to version 3.2 uses 2 20 = 1 M as default).
    piece_length: usize,

    /// pieces maps to a string whose length is a multiple of 20.
    /// It is to be subdivided into strings of length 20,
    /// each of which is the SHA1 hash of the piece at the corresponding index.
    pieces: Vec<u8>,

    /// In the single file case, length maps to the length of the file in bytes.
    length: Option<usize>,

    ///In the multi file case, a set of files which go in a directory structure.
    files: Option<Vec<FilesInfo>>,
}

#[derive(Debug, PartialEq, Clone)]
struct FilesInfo {
    /// The length of the file, in bytes.
    length: usize,

    /// A list of UTF-8 encoded strings corresponding to subdirectory names,
    /// the last of which is the actual file name (a zero length list is an error case).
    path: Vec<String>,
}

fn parse_torrent(data: &[u8]) -> Result<Torrent, String> {
    match parse_bencode(data)? {
        (rest, BencodeValue::Dictionary(dict)) => {
            if !rest.is_empty() {
                return Err("Trailing bytes after root element".to_string());
            }

            let announce = match dict.dict.get(&BString {
                content: b"announce",
            }) {
                Some(BencodeValue::String(BString { content })) => {
                    String::from_utf8(content.to_vec())
                }
                _ => return Err("Invalid or Missing announce url".to_string()),
            }
            .unwrap();

            let info = match dict.dict.get(&BString { content: b"info" }) {
                Some(BencodeValue::Dictionary(dict)) => parse_torrent_info(dict)?,
                _ => return Err("Invalid or Missing torrent info field".to_string()),
            };

            Ok(Torrent { announce, info })
        }
        _ => Err("Expected root element to be a BDictionary".to_string()),
    }
}

fn parse_torrent_info(info: &BDictionary) -> Result<Info, String> {
    let name = match info.dict.get(&BString { content: b"name" }) {
        Some(BencodeValue::String(BString { content })) => String::from_utf8(content.to_vec())
            .map_err(|_| "Invalid utf-8 bytes in name".to_string())?,
        _ => return Err("Invalid or Missing info name".to_string()),
    };

    let piece_length = match info.dict.get(&BString {
        content: b"piece length",
    }) {
        Some(BencodeValue::Integer(BInteger { value })) => *value as usize,
        _ => return Err("Invalid or Missing info piece length".to_string()),
    };

    // get pieces
    let pieces = match info.dict.get(&BString { content: b"pieces" }) {
        Some(BencodeValue::String(BString { content })) => content.to_vec(),
        _ => return Err("Invalid or Missing pieces info".to_string()),
    };

    //get length
    let length = match info.dict.get(&BString { content: b"length" }) {
        Some(BencodeValue::Integer(BInteger { value })) => Some(*value as usize),
        None => None,
        _ => return Err("Invalid info length".to_string()),
    };

    // files exists only if length does not
    let files = if length.is_none() {
        let mut result = Vec::new();

        match info.dict.get(&BString { content: b"files" }) {
            Some(BencodeValue::List(BList { items })) => {
                for fileinfo in items {
                    let file_info = match fileinfo {
                        BencodeValue::Dictionary(BDictionary { dict }) => dict,
                        _ => return Err("fileinfo should be a BDictionary".to_string()),
                    };

                    //get length
                    let length = match file_info.get(&BString { content: b"length" }) {
                        Some(BencodeValue::Integer(BInteger { value })) => *value as usize,
                        _ => return Err("Invalid or MIssing fileinfo length".to_string()),
                    };

                    //get path
                    let mut path = Vec::new();
                    match file_info.get(&BString { content: b"path" }) {
                        Some(BencodeValue::List(BList { items })) => {
                            for item in items {
                                match item {
                                    BencodeValue::String(BString { content }) => {
                                        path.push(String::from_utf8(content.to_vec()).map_err(
                                            |_| "Invalid utf-8 bytes in path".to_string(),
                                        )?)
                                    }
                                    _ => return Err("path item must be a string".to_string()),
                                }
                            }
                        }
                        _ => return Err("Invalid or Missing fileinfo path".to_string()),
                    };

                    result.push(FilesInfo { length, path })
                }
            }
            _ => return Err("Missing or Invalid info files".to_string()),
        }
        Some(result)
    } else {
        None
    };

    Ok(Info {
        name,
        piece_length,
        pieces,
        length,
        files,
    })
}

fn parse_bencode(data: &[u8]) -> Result<(&[u8], BencodeValue), String> {
    match data[0] {
        b'i' => parse_bencode_integer(&data[1..]),
        b'l' => parse_bencode_list(&data[1..]),
        b'd' => parse_bencode_dictionary(&data[1..]),
        b'0'..=b'9' => parse_bencode_string(data),
        _ => Err("Invalid bencode data format".to_string()),
    }
}

fn parse_bencode_string(data: &[u8]) -> Result<(&[u8], BencodeValue), String> {
    let mut i = 0;

    while i < data.len() && data[i] != b':' {
        i += 1;
    }

    if i == data.len() {
        return Err("Invalid BString format".to_string());
    }

    let len_str = String::from_utf8(data[..i].to_vec())
        .map_err(|err| format!("Failed to get len from string: {err}"))?;

    let len = len_str
        .parse::<usize>()
        .map_err(|err| format!("Failed to parse len: {err}"))?;

    if i + 1 + len > data.len() {
        return Err("Missing some String bytes".to_string());
    }

    let content = &data[i + 1..i + 1 + len];

    Ok((
        &data[i + len + 1..],
        BencodeValue::String(BString { content }),
    ))
}

fn parse_bencode_integer(data: &[u8]) -> Result<(&[u8], BencodeValue), String> {
    let mut i = 0;

    while i < data.len() && data[i] != b'e' {
        i += 1;
    }

    let num_str = String::from_utf8(data[..i].to_vec())
        .map_err(|err| format!("Invalid utf-8 bytes in num: {err}"))?;

    let value = num_str
        .parse::<i64>()
        .map_err(|err| format!("failed to parse num: {err}"))?;

    Ok((&data[i + 1..], BencodeValue::Integer(BInteger { value })))
}

fn parse_bencode_list(data: &[u8]) -> Result<(&[u8], BencodeValue), String> {
    let mut items = Vec::new();
    let mut rest = data;

    while !rest.is_empty() && rest[0] != b'e' {
        let (new_rest, value) = parse_bencode(rest)?;
        rest = new_rest;
        items.push(value)
    }

    if rest.is_empty() {
        return Err("Unterminated list".to_string());
    }

    Ok((&rest[1..], BencodeValue::List(BList { items })))
}

fn parse_bencode_dictionary(data: &[u8]) -> Result<(&[u8], BencodeValue), String> {
    let mut rest = data;
    let mut map = HashMap::new();

    while !rest.is_empty() && rest[0] != b'e' {
        //parse key
        let (new_rest, key) = parse_bencode_string(rest)?;
        //key must be string
        let key = match key {
            BencodeValue::String(s) => s,
            _ => return Err("Dictionary key must be BString".to_string()),
        };

        let (new_rest, value) = parse_bencode(new_rest)?;
        rest = new_rest;
        map.insert(key, value);
    }

    if rest.is_empty() {
        return Err("Unterminated Dictionary".to_string());
    }

    Ok((
        &rest[1..],
        BencodeValue::Dictionary(BDictionary { dict: map }),
    ))
}

fn main() -> Result<(), String> {
    let torrent = b"d8:announce11:example.com4:infod4:name9:blindspot12:piece lengthi20e6:pieces5:hello5:filesld6:lengthi10e4:pathl5:path15:path2eeeee";
    let torrent = parse_torrent(torrent)?;
    println!("{:?}", torrent);
    Ok(())
}

#[cfg(test)]
mod test {
    mod bencoding {
        use crate::*;

        #[test]
        fn integer() {
            // 56
            let torrent = b"i56e";

            let (rest, value) = parse_bencode(torrent).unwrap();
            assert_eq!(value, BencodeValue::Integer(BInteger { value: 56 }));
            assert!(rest.is_empty());
        }

        #[test]
        fn string() {
            // "foo"
            let torrent = b"3:foo";

            let (rest, value) = parse_bencode(torrent).unwrap();
            assert!(rest.is_empty());
            assert_eq!(value, BencodeValue::String(BString { content: b"foo" }));
        }

        #[test]
        fn list() {
            // ["foo", "bar"]
            let torrent = b"l3:foo3:bare";

            let (rest, value) = parse_bencode(torrent).unwrap();
            assert!(rest.is_empty());
            assert_eq!(
                value,
                BencodeValue::List(BList {
                    items: [
                        BencodeValue::String(BString { content: b"foo" }),
                        BencodeValue::String(BString { content: b"bar" })
                    ]
                    .to_vec()
                })
            )
        }

        #[test]
        fn simple_dictionary() {
            // {"a": "foo"}
            let torrent = b"d1:a3:fooe";

            let (rest, value) = parse_bencode(torrent).unwrap();
            assert!(rest.is_empty());
            assert_eq!(
                value,
                BencodeValue::Dictionary(BDictionary {
                    dict: HashMap::from([(
                        BString { content: b"a" },
                        BencodeValue::String(BString { content: b"foo" }),
                    ),]),
                }),
            )
        }

        #[test]
        fn simple_dictionary_2() {
            // {"a": "foo", "mike": "angela"}
            let torrent = b"d1:a3:foo4:mike6:angelae";

            let (rest, value) = parse_bencode(torrent).unwrap();
            assert!(rest.is_empty());
            assert_eq!(
                value,
                BencodeValue::Dictionary(BDictionary {
                    dict: HashMap::from([
                        (
                            BString { content: b"a" },
                            BencodeValue::String(BString { content: b"foo" }),
                        ),
                        (
                            BString { content: b"mike" },
                            BencodeValue::String(BString { content: b"angela" })
                        )
                    ]),
                }),
            )
        }

        #[test]
        fn complex_dictionary() {
            // {"foo": "bar", "list": ["angela", "james"]}
            let torrent = b"d3:foo3:bar4:listl6:angela5:jamesee";

            let (rest, map) = parse_bencode(torrent).unwrap();
            assert!(rest.is_empty());
            assert_eq!(
                map,
                BencodeValue::Dictionary(BDictionary {
                    dict: HashMap::from([
                        (
                            BString { content: b"foo" },
                            BencodeValue::String(BString { content: b"bar" })
                        ),
                        (
                            BString { content: b"list" },
                            BencodeValue::List(BList {
                                items: [
                                    BencodeValue::String(BString { content: b"angela" }),
                                    BencodeValue::String(BString { content: b"james" })
                                ]
                                .to_vec()
                            })
                        )
                    ])
                })
            )
        }
    }

    mod torrent {
        use crate::*;
        #[test]
        fn simple() {
            let torrent = b"d8:announce11:example.com4:infod4:name9:blindspot12:piece lengthi20e6:pieces5:hello6:lengthi10eee";
            let torrent = parse_torrent(torrent).unwrap();
            assert_eq!(
                torrent,
                Torrent {
                    announce: "example.com".to_string(),
                    info: Info {
                        name: "blindspot".to_string(),
                        piece_length: 20,
                        pieces: [104, 101, 108, 108, 111].to_vec(),
                        length: Some(10),
                        files: None
                    }
                }
            )
        }

        #[test]
        fn complex() {
            let torrent = b"d8:announce11:example.com4:infod4:name9:blindspot12:piece lengthi20e6:pieces5:hello5:filesld6:lengthi10e4:pathl5:path15:path2eeeee";
            let torrent = parse_torrent(torrent).unwrap();
            assert_eq!(
                torrent,
                Torrent {
                    announce: "example.com".to_string(),
                    info: Info {
                        name: "blindspot".to_string(),
                        piece_length: 20,
                        pieces: [104, 101, 108, 108, 111].to_vec(),
                        length: None,
                        files: Some(
                            [FilesInfo {
                                length: 10,
                                path: ["path1".to_string(), "path2".to_string()].to_vec()
                            }]
                            .to_vec()
                        )
                    }
                }
            )
        }
    }
}
