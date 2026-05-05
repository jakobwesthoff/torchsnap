# Third-Party Asset Attributions

## icon.svg

- **Source:** Simple Icons project,
  https://github.com/simple-icons/simple-icons/blob/develop/icons/zerotier.svg
- **License:** CC0 1.0 Universal (Public Domain Dedication)
- **License text:** https://creativecommons.org/publicdomain/zero/1.0/
- **Modifications:** Split the original single-path SVG into two
  paths to render in two colors. The outer rounded square is filled
  `#ffb25b` (ZeroTier brand orange) and the inner glyph is filled
  `#161c24` with `fill-rule="evenodd"` so the inner network-symbol
  cutouts remain holes. Path coordinates are otherwise unchanged
  (the original second subpath's relative `m-.672 2.834` becomes the
  absolute `M3.338 2.834` of the new path-2 since it no longer
  follows path-1's close).

CC0 does not require attribution; this file is provided as a
courtesy and to document the modification origin for future
maintainers.
