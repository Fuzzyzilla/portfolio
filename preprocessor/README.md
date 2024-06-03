# `preprocessor`
Quick and dirty script to generate a mesh from an `OBJ` file, suitable for `vector-sheepy` to render and simulate.
The imported file should be flat on the XY plane, in the `[-1, 1]` rectangle. It is rendererd `-Y` up, `X` left, `Z` forward.

The renderer does not use depth-buffering, instead this script pre-processes the mesh according to the painter's algorithm.
For best visuals, the mesh should be fairly low poly, as the edges and vertices are visualized as well.

The provided mesh is derived from the SVG provided by [Google Noto Color Emoji `U+1F411`](https://github.com/googlefonts/noto-emoji/blob/41e31b110b4eb929dffb410264694a06205b7ad7/svg/emoji_u1f411.svg?plain=1), and is used under the Apache license.
