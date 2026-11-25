#!/usr/bin/env python3
"""Writes synthetic test data of our own (no game content) into <dir>: `assets/` (a stand-in for the player's import: one ship,
Ship_Test, with a box hull and a sail made in code, its hardpoint nodes, a gun and a bullet, a map of invented scenery) and `mymod/` (a mod that
extends that ship, with a misspelt key the checker flags). For screenshots and scripted runs without the game:

    python3 tools/synthetic_mod.py local/synth
    PROCYON_ASSETS=$PWD/local/synth/assets tools/shot.sh local/3d.png --mod local/synth/mymod --tab 3d --ship Ship_Test

The folder is deleted and written again. Nothing here is copied or converted from the game.
"""
import json, struct, os, sys, shutil
root = sys.argv[1]
shutil.rmtree(root, ignore_errors=True)
def box(cx, cy, cz, sx, sy, sz):
    # game axes centre/size -> glTF (x, z, -y)
    g = lambda x, y, z: (x, z, -y)
    xs, ys, zs = (cx - sx/2, cx + sx/2), (cy - sy/2, cy + sy/2), (cz - sz/2, cz + sz/2)
    v = [g(x, y, z) for x in xs for y in ys for z in zs]
    # 8 corners index i = xi*4 + yi*2 + zi
    faces = [(0,1,3,2),(4,6,7,5),(0,4,5,1),(2,3,7,6),(0,2,6,4),(1,5,7,3)]
    idx = []
    for a,b,c,d in faces:
        idx += [a,b,c, a,c,d]
    return v, idx
meshes, bin_ = [], b""
views, accessors = [], []
def add(v, idx):
    global bin_
    off = len(bin_)
    pb = b"".join(struct.pack("<3f", *p) for p in v)
    ib = b"".join(struct.pack("<H", i) for i in idx)
    ib += b"\0" * ((4 - len(ib) % 4) % 4)
    bin_ += pb + ib
    views.append({"buffer": 0, "byteOffset": off, "byteLength": len(pb), "target": 34962})
    views.append({"buffer": 0, "byteOffset": off + len(pb), "byteLength": len(idx) * 2, "target": 34963})
    mn = [min(p[k] for p in v) for k in range(3)]; mx = [max(p[k] for p in v) for k in range(3)]
    accessors.append({"bufferView": len(views) - 2, "componentType": 5126, "count": len(v), "type": "VEC3", "min": mn, "max": mx})
    accessors.append({"bufferView": len(views) - 1, "componentType": 5123, "count": len(idx), "type": "SCALAR"})
    meshes.append({"primitives": [{"attributes": {"POSITION": len(accessors) - 2}, "indices": len(accessors) - 1}]})
    return len(meshes) - 1
hull = add(*box(0, 0, 0.5, 4, 12, 2))
sail = add(*box(0, 1, 4, 0.2, 3, 5))
g = lambda x, y, z: [x, z, -y]
nodes = [{"name": "Hull_Main", "mesh": hull}, {"name": "Mast_Sail", "mesh": sail}]
for name, pos, kind in [("Gun_Mount_A", (2, 2, 1.5), "GunPlacement"), ("Gun_Mount_B", (-2, 2, 1.5), "GunPlacement"), ("Engine_1", (0, -6.5, 1), None),
                        ("Wake_1", (0, -6, 0), None), ("Flag_Top", (0, 1, 7), None), ("Tow_Front", (0, 6, 1), None), ("Tow_Back", (0, -6, 1), None),
                        ("Homing", (0, 0, 1), None), ("Boarding_1", (1.5, 0, 1.5), None)]:
    n = {"name": name, "translation": g(*pos)}
    if kind: n["extras"] = {"kind": kind}
    nodes.append(n)
doc = {"asset": {"version": "2.0", "generator": "openprocyon-editor synthetic test"}, "scene": 0, "scenes": [{"nodes": list(range(len(nodes)))}],
       "nodes": nodes, "meshes": meshes, "accessors": accessors, "bufferViews": views, "buffers": [{"byteLength": len(bin_)}]}
js = json.dumps(doc).encode(); js += b" " * ((4 - len(js) % 4) % 4)
glb = struct.pack("<III", 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(bin_)) + struct.pack("<II", len(js), 0x4E4F534A) + js + struct.pack("<II", len(bin_), 0x004E4942) + bin_
def put(rel, data):
    p = os.path.join(root, rel); os.makedirs(os.path.dirname(p), exist_ok=True)
    open(p, "wb" if isinstance(data, bytes) else "w").write(data)
put("assets/ships/Ship_Test/model.glb", glb)
put("assets/ships/Ship_Test/ship.json", json.dumps({"race": "Navy", "skirmish": True, "points": 100, "core_hp": 80, "physics": {"Mass": 4000, "MaxSpeed": 8},
    "guns": {"Gun_Mount_A": {"gun": "Gun_Test", "arc_deg": 90}, "Gun_Mount_B": {"gun": "Gun_Test", "arc_deg": 90}},
    "crew_nodes": [{"name": "Crew_Walk_P_0", "pos": [0, -3, 1.6]}, {"name": "Crew_Post_Captain", "pos": [0, -4, 1.6]}]}, indent=2))
put("assets/data/weapons/guns.json", json.dumps({"Gun_Test": {"MuzzleSpeed": 100, "TimeBetweenBursts": 3, "BulletsPerBurst": 1, "DuringBurstReloadTime": 0,
    "EffectiveRange_Range": 100, "LongRange_Range": 150, "MaximumRange_Range": 200, "EffectiveRange_AccuracyDeviation": 0.01, "LongRange_AccuracyDeviation": 0.02,
    "MaximumRange_AccuracyDeviation": 0.03, "BulletAffectedByGravity": False, "BulletTypeName_Primary": "Bullet_Test", "Mount Type": "Medium Mount"}}, indent=2))
put("assets/data/weapons/bullets.json", json.dumps({"Bullet_Test": {"lifetime": 3, "damage": {"Wood": [5, 8, 0, 0, 0]}}}, indent=2))
# Invented scenery, a ship, mines and two starts for the Maps tab's picture; every id is unique and the entries are all keyed.
put("assets/maps/test_map/map.json", json.dumps({"name": "Test", "world_half": 500,
    "objects": [
        {"type": "Test_Island", "id": 1, "scene": "Test_Island", "pos": [-220, 150, 0], "rot": [1, 0, 0, 0, 1, 0, 0, 0, 1]},
        {"type": "Test_Island", "id": 2, "scene": "Test_Island", "pos": [180, 210, 0], "rot": [0.8660254, -0.5, 0, 0.5, 0.8660254, 0, 0, 0, 1]},
        {"type": "Test_Rock", "id": 3, "scene": "Test_Rock", "pos": [20, -60, 0]},
        {"type": "Test_Rock", "id": 4, "scene": "Test_Rock", "pos": [70, -120, 0]},
        {"type": "Test_Base", "id": 5, "scene": "Test_Base", "pos": [-150, -240, 0], "rot": [0, 1, 0, -1, 0, 0, 0, 0, 1]}],
    "ships": [{"id": 6, "type": "Ship_Test", "player": 0, "pos": [-60, 20, 0]}],
    "mines": [{"id": 7, "type": "Bullet_Test", "pos": [120, 40, 0]}, {"id": 8, "type": "Bullet_Test", "pos": [150, 70, 0]}],
    "players": [{"name": "North", "team": 0, "start": [0, 330, 0], "forward": [0, -1, 0], "playable": True}, {"name": "South", "team": 1, "start": [0, -330, 0], "forward": [0, 1, 0], "playable": True}]}, indent=2))
put("mymod/mod.json", json.dumps({"id": "mymod", "name": "Visual test mod", "version": "0.1.0", "description": "synthetic", "category": "Ships", "format": 1, "game_version": "0.2.0"}, indent=2))
put("mymod/ships/Ship_Test/ship.json", json.dumps({"extends": "Ship_Test", "points": 120, "sigth": 300}, indent=2))
print("ok", len(glb), "bytes glb")
