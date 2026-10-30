//! A plugin's world markers through the SDK, against the fake host that
//! keeps them exactly as the client does.

use openac_plugin::testing::FakeHost;
use openac_plugin::ui::{Image, MapPoint, MarkerAnchor, world_icon};

const ART: Image = Image {
    handle: 7,
    width: 32,
    height: 32,
};

#[test]
fn a_plugin_hangs_icons_in_layers_it_owns() {
    let mut host = FakeHost::new("example.markers");
    let (first, second) = host.with_host(|h| {
        let first = h.create_marker_layer();
        let second = h.create_marker_layer();
        let mut framed = world_icon(MarkerAnchor::Object(0x8000_0001), ART);
        framed.border_rgba = Some(0xFF00_00FF);
        let spot = MarkerAnchor::Position(
            MapPoint {
                east_west: 10.0,
                north_south: 20.0,
                elevation: 0.5,
            },
            true,
        );
        assert!(first.set_icons(h, &[framed]));
        assert!(second.set_icons(h, &[world_icon(spot, ART)]));
        (first, second)
    });
    let shown = host.ui.merged_icons();
    assert_eq!(shown.len(), 2);
    assert_eq!(shown[0].owner, "example.markers");
    assert_eq!(shown[0].icon.image, 7);
    assert_eq!(shown[0].icon.size, 24.0);
    assert_eq!(shown[0].icon.tint_rgba, 0xFFFF_FFFF);
    assert_eq!(shown[0].icon.border_rgba, Some(0xFF00_00FF));
    assert_eq!(shown[0].icon.max_range, 60.0);
    assert!(shown[1].icon.anchor.is_valid());
    assert_ne!(
        shown[1].icon.anchor,
        ac_plugin_ui::MarkerAnchor::Object(0x8000_0001)
    );

    assert!(host.with_host(|h| first.remove(h)));
    assert_eq!(host.ui.merged_icons().len(), 1);
    assert!(!host.with_host(|h| first.set_icons(h, &[])));

    // Over the cap across its layers: refused, and the layer keeps its icon.
    let many = vec![world_icon(MarkerAnchor::Object(1), ART); 257];
    assert!(!host.with_host(|h| second.set_icons(h, &many)));
    assert_eq!(host.ui.merged_icons().len(), 1);

    // Leaving the world empties the layer; it stays usable.
    host.ui.clear_markers();
    assert!(host.ui.merged_icons().is_empty());
    assert!(
        host.with_host(|h| { second.set_icons(h, &[world_icon(MarkerAnchor::Object(1), ART)]) })
    );
    assert_eq!(host.ui.merged_icons().len(), 1);
}
