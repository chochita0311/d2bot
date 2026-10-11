# Shared Horadric Cube UI

`templates/transmute-button.png` and `transmute-button-hover-ko.png` preserve the button/hover pixels consumed by gem summing. They are UI states rather than item icons. The actual [selected cube item](../../items/horadric-cube/README.md) has a separate owner.

`references/empty-grid.png` is a retained uncalibrated empty-grid UI catalog example with no current runtime consumer. Its presence does not establish a cube-empty detector or action precondition.

`references/empty-panel-ko-588x757.png` is the generic empty left panel at `[0,0,588,757]` from a 1679×758 cube-plus-inventory source (SHA256 `959895f399192612039e725a5592c8573552e52542624c236fef67cd56cd598a`). It excludes personal equipment/bag content and supplies an actual other-panel negative for stash matching. Its pixels were compared exactly with the source before the unnecessary full personal original was removed (`source_image_retained=false`).
