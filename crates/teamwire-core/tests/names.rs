//! Name rules (LCH-6, INV-32).

use proptest::prelude::*;
use teamwire_core::{Actor, Address, DisplayId, HarnessId, HostId, Recipient, Role, TeamName};

#[test]
fn team_names() {
    for good in ["a", "tw", "team1", "abcdefghijkl"] {
        assert!(TeamName::parse(good).is_ok(), "{good}");
    }
    for bad in [
        "",
        "1team",
        "Team",
        "my-team",
        "my_team",
        "abcdefghijklm",
        "tw ",
        "tëam",
    ] {
        assert!(TeamName::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn roles() {
    for good in ["a", "api", "api_2", "specmsg", "abcdefghijklmnop"] {
        assert!(Role::parse(good).is_ok(), "{good}");
    }
    for bad in [
        "",
        "lead",
        "2api",
        "Api",
        "api-2",
        "abcdefghijklmnopq",
        "_api",
    ] {
        assert!(Role::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn lead_is_refused_as_a_role_with_a_reason() {
    let err = Role::parse("lead").unwrap_err();
    assert!(err.rule.contains("reserved"), "{err}");
}

#[test]
fn addresses() {
    assert!(Address::parse("tw_tw_api").is_ok());
    assert!(Address::parse("tw_abcdefghijkl_abcdefghijklmnop").is_ok());
    for bad in [
        "tw-api",
        "tw_api",
        "tw_tw-x_api",
        "tw_tw_lead",
        "xx_tw_api",
        "tw_abcdefghijklm_abcdefghijklmno",
        "tw_tw_api-2",
    ] {
        assert!(Address::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn adapter_ids() {
    for good in ["claude-code", "herdr", "tmux", "none", "fake-a"] {
        assert!(HarnessId::parse(good).is_ok(), "{good}");
        assert!(HostId::parse(good).is_ok(), "{good}");
    }
    for bad in ["", "Claude", "1x", "claude_code", &"a".repeat(33)] {
        assert!(HarnessId::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn display_ids() {
    assert!(DisplayId::parse("m014").is_ok());
    for bad in ["m14", "014", "M014", "m01x", "m0123456789"] {
        assert!(DisplayId::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn actor_and_recipient_string_forms() {
    assert_eq!(Actor::parse("lead").unwrap(), Actor::Lead);
    let api = Role::parse("api").unwrap();
    assert_eq!(Actor::parse("api").unwrap(), Actor::Member(api.clone()));
    assert_eq!(Recipient::parse("api").unwrap().as_str(), "api");
    assert!(Actor::parse("bad-role").is_err());
    let json = serde_json::to_string(&Recipient::Member(api)).unwrap();
    assert_eq!(json, "\"api\"");
    assert_eq!(
        serde_json::from_str::<Recipient>(&json).unwrap().as_str(),
        "api"
    );
}

#[test]
fn invalid_name_is_refused_when_read_from_json() {
    assert!(serde_json::from_str::<Role>("\"lead\"").is_err());
    assert!(serde_json::from_str::<TeamName>("\"a-b\"").is_err());
}

proptest! {
    #[test]
    fn address_never_contains_hyphen_and_fits_32(
        team in "[a-z][a-z0-9]{0,11}",
        role in "[a-z][a-z0-9_]{0,15}",
    ) {
        prop_assume!(role != "lead");
        let team = TeamName::parse(&team).unwrap();
        let role = Role::parse(&role).unwrap();
        let address = Address::of(&team, &role);
        prop_assert!(!address.as_str().contains('-'));
        prop_assert!(address.as_str().len() <= 32);
        prop_assert_eq!(Address::parse(address.as_str()).unwrap(), address);
    }
}
