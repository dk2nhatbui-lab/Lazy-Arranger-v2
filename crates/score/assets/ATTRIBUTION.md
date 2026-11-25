# Clef images

`treble_clef.png` and `bass_clef.png` are rasterized from public-domain SVG
files on Wikimedia Commons:

- https://commons.wikimedia.org/wiki/File:Treble_clef.svg (PD-self, Tlusťa)
- https://commons.wikimedia.org/wiki/File:Bass_clef.svg (PD-self)

Only the clef outlines were kept; staff lines were removed. The PNGs are
600 px high with transparency.

The all-tracks score (`full.rs`) draws its clefs from the Bravura music font
instead. The font comes bundled with the `cecilia-engrave` crate and is licensed
under the SIL Open Font License.
