//! A plain binary-glTF reader for scenery (docs/phase4.md P7, work item T9): the triangles of a model, node transforms applied, in the
//! glTF axes the engine's loader also keeps (Bevy's axes), with the material's base colour per vertex. Pure and tested headless.
//!
//! The engine's `data::derive::read` is for ships (it reads hardpoint nodes and hit layers); a map's scenery needs only the shape.
//! PROVISIONAL: triangle lists only (strips, fans, lines and points are skipped and counted), float positions only (no quantised or
//! Draco-compressed meshes), the first buffer is the file's own BIN chunk (no external or data-URI buffers), no textures, no skins.

use bevy::math::{Mat4, Quat, Vec3};
use serde_json::Value;

/// The model's triangles as a soup: three vertices per triangle, no sharing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    /// Vertex positions, Bevy axes, node transforms applied.
    pub positions: Vec<[f32; 3]>,
    /// One colour per vertex: the primitive's material `baseColorFactor` (white when it has none).
    pub colors: Vec<[f32; 4]>,
    /// The box around all positions: `[min, max]`; both zero for an empty model.
    pub aabb: [Vec3; 2],
    /// Primitives that were not triangle lists or could not be read, left out.
    pub skipped: usize,
}

impl Model {
    pub fn triangle_count(&self) -> usize {
        self.positions.len() / 3
    }

    /// The triangles as Bevy-axes corner triples.
    pub fn triangles(&self) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        self.positions.as_chunks::<3>().0.iter().map(|t| [Vec3::from(t[0]), Vec3::from(t[1]), Vec3::from(t[2])])
    }

    /// Flat normals, one per vertex (the triangle's own, counter-clockwise front).
    pub fn flat_normals(&self) -> Vec<[f32; 3]> {
        let mut out = Vec::with_capacity(self.positions.len());
        for [a, b, c] in self.triangles() {
            let n = (b - a).cross(c - a).normalize_or_zero().to_array();
            out.extend([n, n, n]);
        }
        out
    }
}

/// More triangles than this are not read (a guard against a file that would eat the memory), PROVISIONAL: a guess.
pub const MAX_TRIANGLES: usize = 4_000_000;

/// The JSON chunk and the BIN chunk (empty when there is none) of a binary glTF.
fn chunks(bytes: &[u8]) -> Result<(Value, &[u8]), String> {
    let u32_at = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if bytes.get(..4) != Some(b"glTF") {
        return Err("not a binary glTF (no glTF magic)".into());
    }
    if u32_at(4) != Some(2) {
        return Err("only glTF version 2 is read".into());
    }
    let (mut at, mut json, mut bin) = (12usize, None, &bytes[0..0]);
    while at + 8 <= bytes.len() {
        let len = u32_at(at).ok_or("truncated chunk header")? as usize;
        let kind = u32_at(at + 4).ok_or("truncated chunk header")?;
        let body = bytes.get(at + 8..at.saturating_add(8).saturating_add(len)).ok_or("a chunk runs past the end of the file")?;
        match kind {
            0x4E4F_534A if json.is_none() => json = Some(serde_json::from_slice::<Value>(body).map_err(|e| format!("JSON chunk: {e}"))?),
            0x004E_4942 if bin.is_empty() => bin = body,
            _ => {}
        }
        at += 8 + len.next_multiple_of(4);
    }
    Ok((json.ok_or("no JSON chunk")?, bin))
}

/// The bytes of accessor `index` as element slices: `(count, stride, bytes from the first element)`.
struct View<'a> {
    count: usize,
    stride: usize,
    data: &'a [u8],
}

fn view<'a>(doc: &Value, bin: &'a [u8], index: usize, component: u64, ncomp: usize) -> Result<View<'a>, String> {
    let acc = doc["accessors"].get(index).ok_or_else(|| format!("no accessor {index}"))?;
    if acc.get("sparse").is_some() {
        return Err("sparse accessors are not read".into());
    }
    if acc["componentType"].as_u64() != Some(component) {
        return Err(format!("accessor {index}: component type {} where {component} is needed", acc["componentType"]));
    }
    let width = match component {
        5121 => 1,
        5123 => 2,
        _ => 4,
    };
    let count = acc["count"].as_u64().ok_or("accessor without count")? as usize;
    let bv = doc["bufferViews"].get(acc["bufferView"].as_u64().ok_or("accessor without bufferView")? as usize).ok_or("no such bufferView")?;
    if bv["buffer"].as_u64().unwrap_or(0) != 0 {
        return Err("only the file's own BIN buffer is read".into());
    }
    let start = bv["byteOffset"].as_u64().unwrap_or(0) as usize + acc["byteOffset"].as_u64().unwrap_or(0) as usize;
    let elem = width * ncomp;
    let stride = bv["byteStride"].as_u64().map_or(elem, |s| s as usize);
    if stride < elem {
        return Err("byteStride smaller than the element".into());
    }
    let needed = if count == 0 { 0 } else { (count - 1).checked_mul(stride).and_then(|n| n.checked_add(elem)).ok_or("accessor too large")? };
    let data = bin.get(start..start.checked_add(needed).ok_or("accessor too large")?).ok_or_else(|| format!("accessor {index} runs past the BIN chunk"))?;
    Ok(View { count, stride, data })
}

fn positions(doc: &Value, bin: &[u8], index: usize) -> Result<Vec<Vec3>, String> {
    let v = view(doc, bin, index, 5126, 3)?;
    let f = |b: &[u8]| f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    Ok((0..v.count)
        .map(|i| {
            let e = &v.data[i * v.stride..];
            Vec3::new(f(&e[0..4]), f(&e[4..8]), f(&e[8..12]))
        })
        .collect())
}

fn indices(doc: &Value, bin: &[u8], index: usize) -> Result<Vec<u32>, String> {
    let ty = doc["accessors"].get(index).and_then(|a| a["componentType"].as_u64()).ok_or("no such index accessor")?;
    let v = view(doc, bin, index, ty, 1)?;
    Ok((0..v.count)
        .map(|i| {
            let e = &v.data[i * v.stride..];
            match ty {
                5121 => u32::from(e[0]),
                5123 => u32::from(u16::from_le_bytes([e[0], e[1]])),
                _ => u32::from_le_bytes([e[0], e[1], e[2], e[3]]),
            }
        })
        .collect())
}

fn node_matrix(node: &Value) -> Mat4 {
    let nums = |k: &str, n: usize| node[k].as_array().filter(|a| a.len() == n).map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect::<Vec<_>>());
    if let Some(m) = nums("matrix", 16) {
        return Mat4::from_cols_slice(&m);
    }
    let t = nums("translation", 3).map_or(Vec3::ZERO, |t| Vec3::new(t[0], t[1], t[2]));
    let r = nums("rotation", 4).map_or(Quat::IDENTITY, |r| Quat::from_xyzw(r[0], r[1], r[2], r[3]).normalize());
    let s = nums("scale", 3).map_or(Vec3::ONE, |s| Vec3::new(s[0], s[1], s[2]));
    Mat4::from_scale_rotation_translation(s, r, t)
}

fn base_color(doc: &Value, primitive: &Value) -> [f32; 4] {
    let factor = primitive["material"].as_u64().and_then(|m| doc["materials"].get(m as usize)).and_then(|m| m["pbrMetallicRoughness"]["baseColorFactor"].as_array());
    match factor {
        Some(f) if f.len() == 4 => [0, 1, 2, 3].map(|i| f[i].as_f64().unwrap_or(1.0) as f32),
        _ => [1.0; 4],
    }
}

struct Reader<'a> {
    doc: &'a Value,
    bin: &'a [u8],
    out: Model,
}

impl Reader<'_> {
    fn primitive(&mut self, p: &Value, to_world: Mat4) -> Result<(), String> {
        if p["mode"].as_u64().unwrap_or(4) != 4 {
            return Err("not a triangle list".into());
        }
        let pos = positions(self.doc, self.bin, p["attributes"]["POSITION"].as_u64().ok_or("no POSITION")? as usize)?;
        let idx: Vec<u32> = match p["indices"].as_u64() {
            Some(i) => indices(self.doc, self.bin, i as usize)?,
            None => (0..pos.len() as u32).collect(),
        };
        let tris = idx.len() / 3;
        if self.out.triangle_count() + tris > MAX_TRIANGLES {
            return Err(format!("more than {MAX_TRIANGLES} triangles"));
        }
        let colour = base_color(self.doc, p);
        // Check every index first: a primitive is taken whole or not at all.
        if idx.iter().any(|&i| i as usize >= pos.len()) {
            return Err("an index points past the vertices".into());
        }
        for &i in &idx[..tris * 3] {
            let w = to_world.transform_point3(pos[i as usize]);
            self.out.positions.push(w.to_array());
            self.out.colors.push(colour);
        }
        Ok(())
    }

    fn node(&mut self, index: usize, parent: Mat4, depth: usize) {
        let Some(node) = self.doc["nodes"].get(index) else { return };
        if depth > 64 {
            return;
        }
        let world = parent * node_matrix(node);
        if let Some(mesh) = node["mesh"].as_u64().and_then(|m| self.doc["meshes"].get(m as usize)) {
            for p in mesh["primitives"].as_array().into_iter().flatten() {
                if self.primitive(p, world).is_err() {
                    self.out.skipped += 1;
                }
            }
        }
        for c in node["children"].as_array().into_iter().flatten().filter_map(Value::as_u64) {
            self.node(c as usize, world, depth + 1);
        }
    }
}

/// Reads the scene's triangles. An error is a file that is not a binary glTF 2 at all; what inside cannot be read is skipped and
/// counted in [`Model::skipped`].
pub fn read(bytes: &[u8]) -> Result<Model, String> {
    let (doc, bin) = chunks(bytes)?;
    let mut r = Reader { doc: &doc, bin, out: Model::default() };
    let nodes = doc["nodes"].as_array().map_or(0, Vec::len);
    let scene = doc["scenes"].get(doc["scene"].as_u64().unwrap_or(0) as usize);
    let roots: Vec<usize> = match scene {
        Some(s) => s["nodes"].as_array().into_iter().flatten().filter_map(Value::as_u64).map(|n| n as usize).collect(),
        // no scene: every node nobody lists as a child
        None => {
            let children: std::collections::BTreeSet<usize> =
                doc["nodes"].as_array().into_iter().flatten().flat_map(|n| n["children"].as_array().into_iter().flatten().filter_map(Value::as_u64)).map(|c| c as usize).collect();
            (0..nodes).filter(|i| !children.contains(i)).collect()
        }
    };
    for n in roots {
        r.node(n, Mat4::IDENTITY, 0);
    }
    let mut m = r.out;
    if let Some(first) = m.positions.first() {
        let (mut lo, mut hi) = (Vec3::from(*first), Vec3::from(*first));
        for p in &m.positions {
            lo = lo.min(Vec3::from(*p));
            hi = hi.max(Vec3::from(*p));
        }
        m.aabb = [lo, hi];
    }
    Ok(m)
}

#[cfg(test)]
pub(crate) mod fixtures {
    use serde_json::{Value, json};

    /// A binary glTF from a JSON document and a BIN chunk.
    pub fn glb(doc: &Value, bin: &[u8]) -> Vec<u8> {
        let mut text = serde_json::to_vec(doc).unwrap();
        while !text.len().is_multiple_of(4) {
            text.push(b' ');
        }
        let mut bin = bin.to_vec();
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let total = 12 + 8 + text.len() + 8 + bin.len();
        let mut out = b"glTF".to_vec();
        out.extend(2u32.to_le_bytes());
        out.extend((total as u32).to_le_bytes());
        out.extend((text.len() as u32).to_le_bytes());
        out.extend(0x4E4F_534Au32.to_le_bytes());
        out.extend(text);
        out.extend((bin.len() as u32).to_le_bytes());
        out.extend(0x004E_4942u32.to_le_bytes());
        out.extend(bin);
        out
    }

    /// A unit cube (side 2, centred on the origin) as 12 indexed triangles, in glTF axes, translated by `at`, with `colour`.
    pub fn cube(at: [f32; 3], colour: [f32; 4]) -> Vec<u8> {
        let mut bin = vec![];
        let corners: Vec<[f32; 3]> = (0..8).map(|i| [if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 }]).collect();
        for c in &corners {
            for v in c {
                bin.extend(v.to_le_bytes());
            }
        }
        let faces: [[u16; 4]; 6] = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
        let mut idx = vec![];
        for [a, b, c, d] in faces {
            idx.extend([a, b, c, a, c, d]);
        }
        for i in &idx {
            bin.extend(i.to_le_bytes());
        }
        let doc = json!({
            "asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "Cube", "mesh": 0, "translation": at}],
            "materials": [{"pbrMetallicRoughness": {"baseColorFactor": colour}}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1, "material": 0}]}],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 8, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5123, "count": 36, "type": "SCALAR"}
            ],
            "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 96}, {"buffer": 0, "byteOffset": 96, "byteLength": 72}],
            "buffers": [{"byteLength": 168}]
        });
        glb(&doc, &bin)
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{cube, glb};
    use super::*;
    use serde_json::json;

    #[test]
    fn an_indexed_cube_is_twelve_triangles_with_its_colour_and_box() {
        let m = read(&cube([10.0, 0.0, -5.0], [0.2, 0.4, 0.6, 1.0])).unwrap();
        assert_eq!((m.triangle_count(), m.skipped), (12, 0));
        assert_eq!(m.colors.len(), m.positions.len());
        assert!(m.colors.iter().all(|c| *c == [0.2, 0.4, 0.6, 1.0]));
        assert_eq!(m.aabb, [Vec3::new(9.0, -1.0, -6.0), Vec3::new(11.0, 1.0, -4.0)], "the node's translation is applied");
        assert_eq!(m.flat_normals().len(), 36);
    }

    #[test]
    fn child_nodes_compose_scale_and_rotation_in_order() {
        // one triangle (0,0,0) (1,0,0) (0,1,0); the parent scales by 2 and moves +x 10; the child turns a quarter about z and moves +y 1
        let mut bin = vec![];
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend(v.to_le_bytes());
        }
        let q = std::f32::consts::FRAC_1_SQRT_2;
        let doc = json!({
            "asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}],
            "nodes": [
                {"scale": [2.0, 2.0, 2.0], "translation": [10.0, 0.0, 0.0], "children": [1]},
                {"rotation": [0.0, 0.0, q, q], "translation": [0.0, 1.0, 0.0], "mesh": 0}
            ],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}],
            "bufferViews": [{"buffer": 0, "byteLength": 36}], "buffers": [{"byteLength": 36}]
        });
        let m = read(&glb(&doc, &bin)).unwrap();
        // child: (x, y) -> (-y, x) + (0, 1); parent: *2 + (10, 0)
        let want = [Vec3::new(10.0, 2.0, 0.0), Vec3::new(10.0, 4.0, 0.0), Vec3::new(8.0, 2.0, 0.0)];
        for (got, want) in m.positions.iter().zip(want) {
            assert!((Vec3::from(*got) - want).length() < 1e-5, "{got:?} vs {want:?}");
        }
        assert_eq!(m.colors[0], [1.0; 4], "no material: white");
    }

    #[test]
    fn what_cannot_be_read_is_skipped_and_counted_never_a_panic() {
        let mut bin = vec![];
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend(v.to_le_bytes());
        }
        let doc = json!({
            "asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0}],
            "meshes": [{"primitives": [
                {"attributes": {"POSITION": 0}},
                {"attributes": {"POSITION": 0}, "mode": 5},
                {"attributes": {"POSITION": 9}},
                {"attributes": {"POSITION": 1}}
            ]}],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 0, "componentType": 5126, "count": 3000, "type": "VEC3"}
            ],
            "bufferViews": [{"buffer": 0, "byteLength": 36}], "buffers": [{"byteLength": 36}]
        });
        let m = read(&glb(&doc, &bin)).unwrap();
        assert_eq!((m.triangle_count(), m.skipped), (1, 3), "a strip, a missing accessor and one running past the data");
    }

    #[test]
    fn an_index_past_the_vertices_drops_the_primitive() {
        let mut bin = vec![];
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend(v.to_le_bytes());
        }
        for i in [0u16, 1, 7, 0] {
            bin.extend(i.to_le_bytes());
        }
        let doc = json!({
            "asset": {"version": "2.0"}, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1}]}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}, {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}],
            "bufferViews": [{"buffer": 0, "byteLength": 36}, {"buffer": 0, "byteOffset": 36, "byteLength": 8}], "buffers": [{"byteLength": 44}]
        });
        let m = read(&glb(&doc, &bin)).unwrap();
        assert_eq!((m.triangle_count(), m.skipped), (0, 1));
    }

    #[test]
    fn a_file_that_is_not_a_binary_gltf_is_an_error() {
        assert!(read(b"").is_err());
        assert!(read(b"nope nope nope nope").is_err());
        let mut v2 = cube([0.0; 3], [1.0; 4]);
        v2[4] = 1;
        assert!(read(&v2).is_err(), "version 1");
        let cut = cube([0.0; 3], [1.0; 4]);
        assert!(read(&cut[..cut.len() - 40]).is_err(), "a chunk runs past the end");
    }
}
