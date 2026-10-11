# Shared gem-count digit atlas

[manifest.json](manifest.json) indexes 420 normalized 12×14 grayscale glyph tiles in [atlas.png](atlas.png) and records the atlas SHA256. It preserves the original six preprocessing variants, ones/tens/blank banks, digit-key order and sample order. Each rebuilt glyph was compared byte-for-byte with the former full-stash builder before that builder was removed.

The atlas contains single generic digit shapes and blanks. It records no source row/column, combined quantities, stash names, character identifiers or personal stock matrix. The source SHA256 and `source_image_retained=false` preserve extraction provenance; the unnecessary full personal stock screenshot was removed after glyph preservation and its original-stash positive replay were verified. The atlas is the shared runtime input.

Gem summing locates the shared [stash header](../stash/templates/header-ko-576x74.png), then reads current slot counts using this atlas. The header retains the generic title/border/close button above the personal tab labels and content. The consumer retains the reviewed full-panel geometry and original matching threshold; the header does not identify the current stash tab or guarantee every count is readable.
