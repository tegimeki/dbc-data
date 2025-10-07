//! DBC Message information

use crate::parse_attr;
use can_dbc::{AttributeValuedForObjectType, MessageId, DBC};
use syn::{Attribute, Field, Ident, Type, Variant};

pub struct MessageInfo<'a> {
    pub id: u32,
    pub extended: bool,
    pub index: usize,
    pub ident: &'a Ident,
    pub cycle_time: Option<usize>,
    signal_list: Vec<String>,
}

impl<'a> MessageInfo<'a> {
    pub fn from_enum_variant(dbc: &DBC, variant: &'a Variant) -> Option<Self> {
        Self::new(dbc, &variant.ident, &variant.attrs)
    }

    pub fn from_struct_field(dbc: &DBC, field: &'a Field) -> Option<Self> {
        let stype = match &field.ty {
            Type::Path(v) => v,
            Type::Array(a) => match *a.elem {
                Type::Path(ref v) => v,
                _ => unimplemented!(),
            },
            _ => unimplemented!(),
        };
        Self::new(dbc, &stype.path.segments[0].ident, &field.attrs)
    }

    fn new(dbc: &DBC, ident: &'a Ident, attrs: &[Attribute]) -> Option<Self> {
        let name = ident.to_string();

        for (index, message) in dbc.messages().iter().enumerate() {
            if message.message_name() == &name {
                let id = message.message_id();
                let (id32, extended) = match *id {
                    MessageId::Standard(id) => (u32::from(id), false),
                    MessageId::Extended(id) => (id, true),
                };

                let cycle_time =
                    Self::message_attr_value(dbc, *id, "GenMsgCycleTime");

                let mut signal_list: Vec<String> = vec![];
                if let Some(attrs) = parse_attr(attrs, "dbc_signals") {
                    let list = attrs.split(',');
                    for name in list {
                        signal_list.push(name.trim().to_string());
                    }
                }

                return Some(Self {
                    id: id32,
                    extended,
                    index,
                    ident,
                    cycle_time,
                    signal_list,
                });
            }
        }
        None
    }

    pub fn use_signal(&self, name: impl Into<String>) -> bool {
        if self.signal_list.is_empty() {
            return true;
        }
        let name = name.into();
        self.signal_list.contains(&name)
    }

    // TODO: revisit this to handle type conversion better; we
    // expect that the value fits in a usize for e.g. GenMsgCycleTime
    fn attr_value(v: &can_dbc::AttributeValue) -> usize {
        use can_dbc::AttributeValue as AV;
        match v {
            AV::AttributeValueU64(x) => *x as usize,
            AV::AttributeValueI64(x) => *x as usize,
            AV::AttributeValueF64(x) => *x as usize,
            AV::AttributeValueCharString(_) => 0usize, // TODO: parse as int?
        }
    }

    fn message_attr_value(
        dbc: &DBC,
        id: MessageId,
        name: &str,
    ) -> Option<usize> {
        for attr in dbc.attribute_values() {
            let value = attr.attribute_value();
            if let AttributeValuedForObjectType::MessageDefinitionAttributeValue(aid, Some(av)) = value {
                if aid == &id && attr.attribute_name() == name {
                    return Some(Self::attr_value(av));
                }
            }
        }
        None
    }
}
