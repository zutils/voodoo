#![allow(unused_imports, dead_code, unused_variables, unused_mut)]

extern crate xml;

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use xml::reader::{EventReader, XmlEvent};
use xml::attribute::OwnedAttribute;

const PRINT: bool = false;


fn convert_type(orig_type: &str) -> String {
    match orig_type {
        "float" => String::from("f32"),
        "int32_t" => String::from("i32"),
        "uint32_t" => String::from("u32"),
        "char" => String::from("i8"),
        "uint8_t" => String::from("u8"),
        "void" => String::from("()"),
        other @ _ => {
            if other.split_at(4).0 != "PFN_" {
                assert!(other.split_at(2).0 == "Vk", "unknown type: {}", other);
            }
            String::from(other)
        }
    }
}


#[derive(Clone, Debug, PartialEq, Eq)]
enum TypeCategory {
    None,
    Struct,
    Union,
    Enum,
    Other,
}


#[derive(Clone, Debug)]
struct Member {
    ty: String,
    name: String,
    is_ptr: bool,
    is_const: bool,
    is_struct: bool,
    optional: bool,
    noautovalidity: bool,
    externsync: bool,
    array_size: Option<String>,
    comment: Option<String>,
    values: Option<String>,
    len: Option<String>,
    altlen: Option<String>,
}

impl Member {
    fn new(attribs: &[OwnedAttribute]) -> Member {
        let mut member = Member {
            ty: String::new(),
            name: String::new(),
            is_ptr: false,
            is_const: false,
            is_struct: false,
            optional: false,
            noautovalidity: false,
            externsync: false,
            array_size: None,
            comment: None,
            values: None,
            len: None,
            altlen: None,
        };

        for attrib in attribs {
            match attrib.name.local_name.as_str() {
                "values" => member.values = Some(attrib.value.clone()),
                "optional" => member.optional |= attrib.value == "true",
                "len" => member.len = Some(attrib.value.clone()),
                "noautovalidity" => member.noautovalidity |= attrib.value == "true",
                "altlen" => member.altlen = Some(attrib.value.clone()),
                "externsync" => member.externsync |= attrib.value == "true",
                unknown @ _ => panic!("unknown struct attribute: {:?}={:?}",
                    unknown, attrib.value),
            }
        }

        member
    }

    fn validate(&self) {
        assert!(self.name.len() > 0);
    }
}


#[derive(Clone, Debug)]
struct Struct {
    name: String,
    returnedonly: bool,
    structextends: Option<String>,
    comment: String,
    members: Vec<Member>,
}

impl Struct {
    fn new(attribs: &[OwnedAttribute]) -> Struct {
        let mut name = None;
        let mut returnedonly = false;
        let mut structextends = None;
        let mut comment = String::new();

        for attrib in attribs {
            match attrib.name.local_name.as_str() {
                "category" => (),
                "name" => name = Some(attrib.value.clone()),
                "returnedonly" => returnedonly |= attrib.value == "true",
                "structextends" => structextends = Some(String::from(attrib.value.clone())),
                "comment" => comment = attrib.value.clone(),
                unknown @ _ => panic!("unknown struct attribute: {:?}={:?}",
                    unknown, attrib.value),
            }
        }

        Struct {
            name: name.expect("no struct name found"),
            returnedonly,
            structextends,
            comment,
            members: Vec::with_capacity(16),
        }
    }
}


fn category(s: &str) -> TypeCategory {
    match s {
        "struct" => TypeCategory::Struct,
        "union" => TypeCategory::Union,
        "enum" => TypeCategory::Enum,
        _ => TypeCategory::Other,
    }
}

fn parse_stray_text(s: &str, current_member: &mut Member) {
    match s {
        "[" => (),
        "]" => (),
        "[2]" => current_member.array_size = Some("2".to_string()),
        "[4]" => current_member.array_size = Some("4".to_string()),
        _ => {
            if s.starts_with("const") {
                current_member.is_const = true;
            } else if s.starts_with("struct") {
                current_member.is_struct = true;
            } else if s.starts_with("*") {
                current_member.is_ptr = true;
            } else if s.starts_with("[") {
                let mut array_size = String::with_capacity(4);
                for (char_idx, c) in s.chars().enumerate() {
                    match c {
                        '[' => (),
                        ']' => assert!(char_idx == s.len() - 1),
                        digit @ _ => {
                            assert!(digit.is_numeric(),
                                "unexpected character found \
                                while parsing array size: {}", c);
                            array_size.push(digit);
                        },
                        // _ => panic!(),
                    }
                }

            } else {
                panic!("unknown characters present: {}", s)
            }
        }
    }
}

fn indent(size: usize) -> String {
    const INDENT: &'static str = "    ";
    (0..size).map(|_| INDENT)
        .fold(String::with_capacity(size*INDENT.len()), |r, s| r + s)
}

fn main() {
    let file = File::open("./gen_src/vk.xml").unwrap();
    let reader = BufReader::new(file);
    let parser = EventReader::new(reader);

    let mut structs: Vec<Struct> = Vec::with_capacity(400);

    let mut current_struct: Option<Struct> = None;
    let mut struct_start_depth = 0;
    let mut parsing_struct_comment = false;

    let mut current_member: Option<Member> = None;
    let mut member_start_depth = 0;
    let mut parsing_member_type = false;
    let mut parsing_member_name = false;
    let mut parsing_member_array_size = false;
    let mut parsing_member_comment = false;

    let mut depth = 0;

    for e in parser {
        match e {
            Ok(XmlEvent::StartElement { name, attributes, .. }) => {
                let mut type_category = TypeCategory::None;

                if name.local_name == "type" {
                    for attrib in &attributes {
                        if attrib.name.local_name == "category" {
                            type_category = category(&attrib.value);
                        }
                    }
                }
                if type_category == TypeCategory::Struct {
                    current_struct = Some(Struct::new(&attributes));
                    struct_start_depth = depth;
                }

                if let Some(ref mut st) = current_struct {
                    match name.local_name.as_str() {
                        "member" => {
                            assert!(current_member.is_none());
                            current_member = Some(Member::new(&attributes));
                            member_start_depth = depth;
                        },
                        "type" => {
                            parsing_member_type = true;
                        },
                        "name" => {
                            parsing_member_name = true;
                        },
                        "enum" => {
                            parsing_member_array_size = true;
                        },
                        "comment" => {
                            if current_member.is_some() {
                                parsing_member_comment = true;
                            } else {
                                parsing_struct_comment = true;
                            }
                        },
                        unknown @ _ => panic!("unknown tag: \"{}\"", unknown),
                    }

                    if PRINT {
                        print!("{}<{}", indent(depth), name);
                        for attrib in attributes {
                            print!(" {}=\"{}\"", attrib.name, attrib.value);
                        }
                        print!(">\n");
                    }
                }
                depth += 1;
            },
            Ok(XmlEvent::EndElement { name }) => {
                depth -= 1;
                if PRINT && current_struct.is_some() {
                    println!("{}</{}>", indent(depth), name);
                }
                if name.local_name == "member" && current_struct.is_some() {
                    if depth == member_start_depth {
                        let st = current_struct.as_mut().expect("no current struct");
                        let new_member = current_member.take().expect("no current member");
                        new_member.validate();
                        st.members.push(new_member);
                    }
                } else if name.local_name == "type" && current_struct.is_some() {
                    if depth == struct_start_depth {
                        assert!(current_struct.is_some());
                        if let Some(st) = current_struct.take() {
                            structs.push(st);;
                        }
                    }
                }
            },
            Ok(XmlEvent::Characters(s)) => {
                if PRINT && current_struct.is_some() {
                    println!("{}{}", indent(depth), s.as_str());
                }
                if let Some(ref mut cur_mem) = current_member {
                    if s.len() > 0 {
                        if parsing_member_type {
                            cur_mem.ty = s;
                            parsing_member_type = false;
                        } else if parsing_member_name {
                            cur_mem.name = s;
                            parsing_member_name = false;
                        } else if parsing_member_array_size {
                            cur_mem.array_size = Some(s);
                            parsing_member_array_size = false;
                        } else if parsing_member_comment {
                            cur_mem.comment = Some(s);
                            parsing_member_comment = false;
                        } else {
                            parse_stray_text(&s, cur_mem);
                        }
                    }
                } else if let Some(ref mut cur_struct) = current_struct {
                    if parsing_struct_comment && s.len() > 0 {
                        cur_struct.comment = String::from(s);
                        parsing_struct_comment = false;
                    }
                }
            },
            // Ok(XmlEvent::CData(s)) => println!("{}{}", indent(depth), s),
            // Ok(XmlEvent::Comment(s)) => println!("{}{}", indent(depth), s),
            // Ok(XmlEvent::Whitespace(s)) => println!("{}{}", indent(depth), s),
            Err(e) => {
                println!("Error: {}", e);
                break;
            },
            _ => {}
        }
    }

    println!("Structs: \n\n{:#?}", structs);
    println!("{} structs parsed", structs.len());
}
