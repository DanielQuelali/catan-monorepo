use fastcore::actions::{ActionCode, ActionKind};

#[test]
fn action_codec_round_trip() {
    let code = ActionCode::new(ActionKind::BuildRoad, 42);
    assert_eq!(code.kind(), ActionKind::BuildRoad);
    assert_eq!(code.payload(), 42);
}
