use std::{hash::Hash, io::BufRead};

struct OrderlessPair<T: Hash + Ord>(T, T);
impl<T: Hash + Ord + PartialEq> PartialEq for OrderlessPair<T> {
    fn eq(&self, other: &Self) -> bool {
        (&self.0).min(&self.1) == (&other.0).min(&other.1)
            && (&self.1).max(&self.0) == (&other.1).max(&other.0)
    }
}
impl<T: Hash + Ord + Eq> Eq for OrderlessPair<T> {}
impl<T: Hash + Ord> Hash for OrderlessPair<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let min = (&self.0).min(&self.1);
        let max = (&self.1).max(&self.0);
        min.hash(state);
        max.hash(state);
    }
}

pub struct FullVertex {
    pub pos: [f32; 3],
    pub color: [u8; 4],
}
pub struct NormVertex {
    pub pos: [u16; 2],
    pub color: [u8; 4],
}

// Read nonstandard obj, with vertex format of `v x y z r g b`.
// Returns (vertices, triangle indices)
pub fn read_nonstandard_obj(
    r: &mut impl BufRead,
) -> anyhow::Result<(Vec<NormVertex>, Vec<[u16; 3]>)> {
    let mut vertices = vec![];
    let mut indices = vec![];
    for line in r.lines() {
        let line = line?;
        match line.split_once(' ') {
            Some(("v", content)) => {
                let mut args = content.split(' ');
                // greedily parse floats
                let vals: Vec<_> = args.map_while(|v| v.parse::<f32>().ok()).collect();
                // Check that we got enough for x,y,z,r,g,b
                if vals.len() < 6 {
                    anyhow::bail!("Not enough data in vertex {content}");
                }
                // Construct and push
                let v = FullVertex {
                    pos: [vals[0], vals[1], vals[2]],
                    color: [
                        (vals[3].clamp(0.0, 1.0) * 255.0) as u8,
                        (vals[4].clamp(0.0, 1.0) * 255.0) as u8,
                        (vals[5].clamp(0.0, 1.0) * 255.0) as u8,
                        255,
                    ],
                };
                vertices.push(v);
            }
            Some(("f", content)) => {
                let mut args = content.split(' ');
                // greedily parse u16s
                let vals: Vec<_> = args.map_while(|v| v.parse::<u16>().ok()).collect();
                // Check that we got enough for face
                if vals.len() < 3 {
                    anyhow::bail!("Not enough data in face {content}");
                }
                indices.push([vals[0]-1, vals[1]-1, vals[2]-1]);
            }
            Some((ty, _)) => {
                println!("Ignored attrib {ty}");
            }
            None => (),
        }
    }

    // HELLA EXPENSIVE OPERATION - that's okey, this is all offline :>
    // Sort all triangles based on painter's algorithm, so that farther elements are earlier in the index list.
    // (removes the need for a depth buffer)
    indices.sort_by(|&[a, ..], &[b, ..]| {
        vertices[a as usize].pos[1].total_cmp(&vertices[b as usize].pos[1])
    });

    let mut extents_x = (100000.0f32, -100000.0f32);
    let mut extents_y = (100000.0f32, -100000.0f32);
    for v in vertices.iter() {
        extents_x.0 = extents_x.0.min(v.pos[0]);
        extents_x.1 = extents_x.1.max(v.pos[0]);

        extents_y.0 = extents_y.0.min(v.pos[2]);
        extents_y.1 = extents_y.1.max(v.pos[2]);
    }
    println!("Found minima {}, {}", extents_x.0, extents_y.0);
    println!("Found maxima {}, {}", extents_x.1, extents_y.1);
    let vertices_norm: Vec<_> = vertices
        .into_iter()
        .map(|v| NormVertex {
            pos: [
                normalize(extents_x.0, extents_x.1, v.pos[0]),
                normalize(extents_y.0, extents_y.1, v.pos[2]),
            ],
            color: v.color,
        })
        .collect();
    Ok((vertices_norm, indices))
}

pub fn normalize(min: f32, max: f32, val: f32) -> u16 {
    let norm = (val.clamp(min, max) - min) / (max-min);
    (norm * (u16::MAX as f32)) as u16
}

pub fn main() -> anyhow::Result<()> {
    let (verts, indices) = read_nonstandard_obj(&mut std::io::BufReader::new(
        std::fs::File::open("/home/aspen/Documents/Blender Projects/baa mesh/baa.obj")?,
    ))?;

    print!("const VERTICES : &'static [NormVertex] = &[");
    for v in verts.iter() {
        print!(
            "NormVertex{{pos:[{},{}],color:[{},{},{},{}]}},",
            v.pos[0], v.pos[1], v.color[0], v.color[1], v.color[2], v.color[3]
        );
    }
    println!("];");

    println!("\n// ====================================\n");

    print!("const EDGES : &'static [[u16; 2]] = &[");
    let mut used_edges = std::collections::HashSet::<OrderlessPair<u16>>::new();
    for &[a, b, c] in indices.iter() {
        [
            OrderlessPair(a, b),
            OrderlessPair(b, c),
            OrderlessPair(a, c),
        ]
        .into_iter()
        .for_each(|pair| {
            if !used_edges.contains(&pair) {
                //New pair found!
                print!("[{}, {}],", pair.0, pair.1);
                used_edges.insert(pair);
            }
        });
    }
    println!("];");

    println!("\n// ====================================\n");

    print!("const FACES : &'static [[u16; 3]] = &[");
    for [a, b, c] in indices.iter() {
        print!("[{}, {}, {}],", a, b, c);
    }
    println!("];");


    let mut size = 0usize;
    size += verts.len() * std::mem::size_of::<NormVertex>();
    size += used_edges.len() * std::mem::size_of::<u16>() * 2;
    size += indices.len() * std::mem::size_of::<u16>() * 3;
    println!("// Processed size: {size} Bytes");

    Ok(())
}
