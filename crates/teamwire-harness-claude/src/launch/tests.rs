use teamwire_core::{OtelConfig, Role, TeamName};

use super::{header_value, otel_vars};

fn names() -> (TeamName, Role) {
    (
        TeamName::parse("demo").unwrap(),
        Role::parse("api").unwrap(),
    )
}

#[test]
fn existing_resource_attributes_come_first() {
    let (team, role) = names();
    let vars = otel_vars(&team, &role, &OtelConfig::default(), Some("service.name=x"));
    assert_eq!(
        vars,
        [(
            "OTEL_RESOURCE_ATTRIBUTES".to_owned(),
            "service.name=x,teamwire.team=demo,teamwire.role=api".to_owned()
        )]
    );
}

#[test]
fn empty_existing_attributes_add_no_leading_comma() {
    let (team, role) = names();
    let vars = otel_vars(&team, &role, &OtelConfig::default(), Some(""));
    assert_eq!(
        vars,
        [(
            "OTEL_RESOURCE_ATTRIBUTES".to_owned(),
            "teamwire.team=demo,teamwire.role=api".to_owned()
        )]
    );
}

#[test]
fn header_values_escape_the_list_separators() {
    assert_eq!(header_value("a,b%c"), "a%2Cb%25c");
}
