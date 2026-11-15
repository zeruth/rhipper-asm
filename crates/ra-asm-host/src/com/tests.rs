use super::*;

#[test]
fn every_interface_begins_with_the_slots_of_iunknown() {
    for interface in INTERFACES {
        let slots = interface.slots();
        let names: Vec<&str> = slots.iter().take(3).map(|m| m.name).collect();
        assert_eq!(names, ["QueryInterface", "AddRef", "Release"], "{}", interface.name);
    }
    assert_eq!(interface("IMalloc").map(|i| i.slots().len()), Some(9));
}

#[test]
fn every_prototype_is_one_the_thunks_read() {
    for interface in INTERFACES {
        for method in interface.slots() {
            let (result, params) = method.kinds.split_at(1);
            let what = format!("{}::{}", interface.name, method.name);
            assert!("viusp".contains(result), "{what}: the result {result:?}");
            assert!(params.chars().all(|k| "iuspgo".contains(k)), "{what}: the parameters {params:?}");
            assert!(params.len() <= 7, "{what}: more parameters than the host calls with");
            if let Some(out) = params.find('o') {
                assert!(params[..out].contains('g'), "{what}: the interface it hands back has no identifier before it");
            }
        }
    }
}

#[test]
fn the_identifiers_are_the_sdks() {
    assert_eq!(iid_text(&IUNKNOWN.iid), "{00000000-0000-0000-C000-000000000046}");
    assert_eq!(iid_text(&IMALLOC.iid), "{00000002-0000-0000-C000-000000000046}");
    assert_eq!(by_iid(&IMALLOC.iid).map(|i| i.name), Some("IMalloc"));
}
