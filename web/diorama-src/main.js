// Mumbai street corner at dusk, drawn as tilt-shift ASCII. Built on the systems in
// https://github.com/JDhruv14/ascii-diorama (MIT): glyph pass, tilt-shift, wind, instanced fill, sea shader.
// Everything is procedural (no textures, no models). export start(host) -> { canvas, play(on), resize() }.
import * as THREE from 'three';
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js';

const DURATION = 26;
const CLS = { street: 1, leaf: 2, pave: 3, stone: 4, dash: 5, kiosk: 6, wall: 7, roof: 8, water: 9, foam: 10, person: 11, glow: 12, coin: 13, vault: 14, auto: 15 };

const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
const sstep = (a, b, x) => { const t = clamp((x - a) / (b - a)); return t * t * (3 - 2 * t); };
const fract = x => x - Math.floor(x);
const hash2 = (x, y) => fract(Math.sin(x * 127.1 + y * 311.7) * 43758.5453);
function vnoise(x, y) {
  const i = Math.floor(x), j = Math.floor(y), f = x - i, g = y - j, u = f * f * (3 - 2 * f), v = g * g * (3 - 2 * g);
  return (hash2(i, j) * (1 - u) + hash2(i + 1, j) * u) * (1 - v) + (hash2(i, j + 1) * (1 - u) + hash2(i + 1, j + 1) * u) * v;
}
const fbm = (x, y) => vnoise(x, y) * .5 + vnoise(x * 2.1 + 3, y * 2.1) * .28 + vnoise(x * 4.3, y * 4.3 + 5) * .15 + vnoise(x * 8.7, y * 8.7) * .07;
let seed = 41;
const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;

const time = { value: 0 };
function tag(mat, cls, extra) {
  mat.onBeforeCompile = (s) => {
    extra?.(s);
    s.fragmentShader = s.fragmentShader.replace('#include <dithering_fragment>', `#include <dithering_fragment>\n gl_FragColor.a = ${(cls / 16).toFixed(4)};`);
  };
  mat.customProgramCacheKey = () => `c${cls}${extra ? 'x' : ''}`;
  return mat;
}
function colored(g, hex) {
  const c = new THREE.Color(hex);
  g = g.index ? g.toNonIndexed() : g; g.deleteAttribute('uv');
  const n = g.attributes.position.count, a = new Float32Array(n * 3);
  for (let i = 0; i < n; i++) a.set([c.r, c.g, c.b], i * 3);
  g.setAttribute('color', new THREE.BufferAttribute(a, 3));
  return g;
}
const vcMat = (cls, o = {}) => tag(new THREE.MeshLambertMaterial({ vertexColors: true, ...o }), cls);

// ---------- layout ----------
const seaZ = x => 7.4 + .7 * Math.sin(x * .18 + 1) + .3 * Math.sin(x * .47);
const ROAD = 3.9; // seafront road centre sits this far inland of the shore
const groundY = (x, z) => { const d = z - seaZ(x); return d < 0 ? 0 : -Math.min(1.6, d * 1.4); };
const PATH = [[-17, -24], [-11.5, -13.5], [-6, -6.2], [-1.5, -1.3], [3, 1.6], [8, 3.0], [13, 3.4]];
const curve = new THREE.CatmullRomCurve3(PATH.map(([x, z]) => new THREE.Vector3(x, 0, z)), false, 'catmullrom', .5);
const LEN = curve.getLength();
const PS = Array.from({ length: 301 }, (_, i) => curve.getPointAt(i / 300));
const X0 = -40, X1 = 40, Z0 = -45, Z1 = 35, FN = 320;
const fD = new Float32Array(FN * FN), fS = new Float32Array(FN * FN);
for (let j = 0; j < FN; j++) for (let i = 0; i < FN; i++) {
  const x = X0 + (X1 - X0) * i / (FN - 1), z = Z0 + (Z1 - Z0) * j / (FN - 1);
  let best = 1e9, bk = 0;
  for (let k = 0; k < PS.length; k++) { const d = (PS[k].x - x) ** 2 + (PS[k].z - z) ** 2; if (d < best) { best = d; bk = k; } }
  fD[j * FN + i] = Math.sqrt(best); fS[j * FN + i] = bk / 300;
}
function samp(f, x, z) {
  const u = clamp((x - X0) / (X1 - X0)) * (FN - 1), v = clamp((z - Z0) / (Z1 - Z0)) * (FN - 1);
  const i = Math.min(FN - 2, u | 0), j = Math.min(FN - 2, v | 0), a = u - i, b = v - j;
  return (f[j * FN + i] * (1 - a) + f[j * FN + i + 1] * a) * (1 - b) + (f[(j + 1) * FN + i] * (1 - a) + f[(j + 1) * FN + i + 1] * a) * b;
}
const streetD = (x, z) => samp(fD, x, z);
const roadD = (x, z) => Math.abs(z - (seaZ(x) - ROAD));
const SW = 1.9, RW = 1.35; // half widths of the diagonal street and the seafront road

// focal points: kiosk on the far side of the street, vault on the near side
const atPath = (u, off) => { const p = curve.getPointAt(u), t = curve.getTangentAt(u); return { x: p.x + t.z * off, z: p.z - t.x * off, nx: -t.z * Math.sign(off), nz: t.x * Math.sign(off) }; };
const KIOSK = atPath(.5, 3.7), VAULT = atPath(.36, -4.3);
const near = (p, x, z, r) => (p.x - x) ** 2 + (p.z - z) ** 2 < r * r;

// ---------- ground ----------
function buildGround() {
  const TW = 640, col = new Uint8Array(TW * TW * 4), cls = new Uint8Array(TW * TW * 4);
  for (let j = 0; j < TW; j++) for (let i = 0; i < TW; i++) {
    const x = X0 + (X1 - X0) * i / (TW - 1), z = Z0 + (Z1 - Z0) * j / (TW - 1);
    const sd = streetD(x, z), rd = roadD(x, z), dz = z - seaZ(x), n = fbm(x * .35, z * .35);
    let c, k;
    if (dz > 0) { c = [70, 74, 86]; k = CLS.stone; }
    else if (dz > -2.5) { // promenade
      const tile = (fract(x / .9) < .08 || fract(z / .9) < .08) ? .8 : 1;
      c = [214 * tile, 196 * tile, 168 * tile]; k = CLS.pave;
    } else if (sd < SW || rd < RW) { // asphalt + dashed centre lines
      const v = .8 + .4 * n; c = [84 * v, 88 * v, 108 * v]; k = CLS.street;
      const s = samp(fS, x, z) * LEN;
      if ((sd < .09 && fract(s / 1.4) < .5 && rd > RW) || (rd < .09 && fract(x / 1.4) < .5 && sd > SW)) { c = [236, 226, 200]; k = CLS.dash; }
    } else if (sd < SW + 1.2 || rd < RW + 1.1 || near(KIOSK, x, z, 4) || near(VAULT, x, z, 3.6)) { // pavement
      const chk = (Math.floor(x / .7) + Math.floor(z / .7)) & 1, v = (.85 + .25 * n) * (chk ? 1 : .9);
      c = [168 * v, 150 * v, 132 * v]; k = CLS.pave;
    } else { const v = .7 + .5 * n; c = [118 * v, 98 * v, 86 * v]; k = CLS.pave; } // lanes between blocks
    const o = (j * TW + i) * 4;
    col[o] = Math.min(255, c[0]); col[o + 1] = Math.min(255, c[1]); col[o + 2] = Math.min(255, c[2]); col[o + 3] = 255; cls[o] = k * 16;
  }
  const colTex = new THREE.DataTexture(col, TW, TW, THREE.RGBAFormat);
  colTex.colorSpace = THREE.SRGBColorSpace; colTex.minFilter = THREE.LinearMipmapLinearFilter; colTex.magFilter = THREE.LinearFilter; colTex.generateMipmaps = true; colTex.anisotropy = 8; colTex.needsUpdate = true;
  const clsTex = new THREE.DataTexture(cls, TW, TW, THREE.RGBAFormat); clsTex.magFilter = clsTex.minFilter = THREE.NearestFilter; clsTex.needsUpdate = true;
  const g = new THREE.PlaneGeometry(X1 - X0, Z1 - Z0, 240, 240); g.rotateX(-Math.PI / 2); g.translate((X0 + X1) / 2, 0, (Z0 + Z1) / 2);
  const p = g.attributes.position, uv = g.attributes.uv;
  for (let i = 0; i < p.count; i++) { const x = p.getX(i), z = p.getZ(i); p.setY(i, groundY(x, z)); uv.setXY(i, (x - X0) / (X1 - X0), (z - Z0) / (Z1 - Z0)); }
  g.computeVertexNormals();
  const m = new THREE.MeshLambertMaterial({ map: colTex });
  m.onBeforeCompile = (s) => {
    s.uniforms.clsTex = { value: clsTex };
    s.fragmentShader = 'uniform sampler2D clsTex;\n' + s.fragmentShader.replace('#include <dithering_fragment>', `#include <dithering_fragment>\n gl_FragColor.a = texture2D(clsTex, vMapUv).r;`);
  };
  const mesh = new THREE.Mesh(g, m); mesh.receiveShadow = true;
  return mesh;
}

// ---------- sea: Arabian Sea at dusk ----------
function buildSea() {
  const m = new THREE.ShaderMaterial({
    uniforms: { uT: time },
    vertexShader: `varying vec3 vW; void main(){ vW = (modelMatrix*vec4(position,1.)).xyz; gl_Position = projectionMatrix*viewMatrix*vec4(vW,1.); }`,
    fragmentShader: /* glsl */`
      uniform float uT; varying vec3 vW;
      float h21(vec2 p){ return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453); }
      float vn(vec2 p){ vec2 i=floor(p),f=fract(p); f=f*f*(3.-2.*f);
        return mix(mix(h21(i),h21(i+vec2(1,0)),f.x),mix(h21(i+vec2(0,1)),h21(i+vec2(1,1)),f.x),f.y); }
      float fbm(vec2 p){ return vn(p)*.55+vn(p*2.1+3.1)*.3+vn(p*4.3+7.7)*.15; }
      float seaZ(float x){ return 7.4+.7*sin(x*.18+1.)+.3*sin(x*.47); }
      void main(){
        float dd = max((vW.z-seaZ(vW.x))*.55,0.);
        float n = fbm(vW.xz*.32+vec2(uT*.08,uT*.05));
        float dist = sqrt(dd);
        float saw = fract((dist*7.2-uT*1.05+n*3.6)/6.2832);
        float lace = smoothstep(.36,.6,fbm(vW.xz*1.25+vec2(uT*.2,-uT*.1)));
        float range = smoothstep(3.6,1.2,dist)*smoothstep(.0,.3,dist);
        float trail = smoothstep(0.,.03,saw)*(1.-smoothstep(.04,.34,saw));
        float foam = trail*range*mix(.3,1.,lace);
        float swash = .25+.16*sin(uT*.8+vW.x*.25+n*5.);
        float rim = smoothstep(swash+.3,swash,dd)*(.45+.55*lace);
        vec3 shallow = vec3(.32,.42,.62), mid = vec3(.07,.16,.5), deep = vec3(.02,.05,.2);
        vec3 c = mix(shallow,mid,smoothstep(.05,1.4,dist));
        c = mix(c,deep,smoothstep(1.4,4.,dist));
        c *= .86+.3*vn(vW.xz*vec2(.9,.5)+uT*vec2(.2,.32));
        float glint = pow(vn(vW.xz*vec2(.7,2.6)+uT*vec2(.25,.4)),3.);
        c += vec3(.95,.42,.18)*glint*smoothstep(1.,3.5,dist)*.9; // sunset on the water
        float cls = 9.;
        float f = max(foam,rim);
        if(f>.3){ c = mix(c,vec3(1.05,1.,.92),clamp(f*1.6,0.,1.)); cls = 10.; }
        gl_FragColor = vec4(c,cls/16.);
      }`,
  });
  const g = new THREE.PlaneGeometry(X1 - X0, 30); g.rotateX(-Math.PI / 2); g.translate(0, -.45, 22);
  return new THREE.Mesh(g, m);
}

// ---------- city blocks: rooftops fill the top band ----------
function buildBlocks(scene) {
  const walls = [], roofs = [], wins = [], spots = [];
  const roofPal = ['#9c7a6a', '#d9cbb0', '#8d8f99', '#b29a86', '#6f7480', '#e6dcc8'], wallPal = ['#b8ad9c', '#9c968e', '#c8bca6', '#9aa0ae'];
  for (let gz = Z0 + 6; gz < 7; gz += 3.3) for (let gx = X0 + 4; gx < X1 - 4; gx += 3.3) {
    const x = gx + (rnd() - .5) * .6, z = gz + (rnd() - .5) * .6;
    if (streetD(x, z) < SW + 3.2 || roadD(x, z) < RW + 2.4 || z > seaZ(x) - 3 || near(KIOSK, x, z, 5.2) || near(VAULT, x, z, 4.6)) { spots.push([x, z]); continue; }
    const w = 2.2 + rnd() * .8, d = 2.2 + rnd() * .8, h = (1 + rnd() * 2.6) * (z < -8 ? 1.5 : 1);
    const b = new THREE.BoxGeometry(w, h, d); b.translate(x, h / 2, z); walls.push(colored(b, wallPal[(rnd() * wallPal.length) | 0]));
    const r = new THREE.BoxGeometry(w + .1, .14, d + .1); r.translate(x, h + .07, z); roofs.push(colored(r, roofPal[(rnd() * roofPal.length) | 0]));
    if (rnd() < .6) { const t = new THREE.CylinderGeometry(.32, .32, .55, 10); t.translate(x + (rnd() - .5) * w * .5, h + .4, z + (rnd() - .5) * d * .5); roofs.push(colored(t, '#2c4fd0')); } // blue water tank
    if (rnd() < .5) { const a = new THREE.BoxGeometry(.4, .25, .3); a.translate(x - w * .3, h + .27, z + d * .3); roofs.push(colored(a, '#cfd2d8')); }
    for (let wy = .55; wy < h - .3; wy += .7) for (let wx = -w / 2 + .45; wx < w / 2 - .3; wx += .6) if (rnd() < .45) wins.push([x + wx, wy, z + d / 2 + .01]);
  }
  const wm = new THREE.Mesh(mergeGeometries(walls), vcMat(CLS.wall)); wm.castShadow = wm.receiveShadow = true; scene.add(wm);
  const rm = new THREE.Mesh(mergeGeometries(roofs), vcMat(CLS.roof)); rm.castShadow = rm.receiveShadow = true; scene.add(rm);
  const im = new THREE.InstancedMesh(new THREE.PlaneGeometry(.26, .3), tag(new THREE.MeshBasicMaterial({ color: new THREE.Color(1.25, .86, .5) }), CLS.glow), wins.length), o = new THREE.Object3D();
  wins.forEach(([x, y, z], i) => { o.position.set(x, y, z); o.updateMatrix(); im.setMatrixAt(i, o.matrix); });
  scene.add(im);
  return spots;
}

// ---------- trees: street trees and courtyard rain trees ----------
function leafWind(s) {
  s.uniforms.uT = time;
  s.vertexShader = 'uniform float uT;\n' + s.vertexShader.replace('#include <begin_vertex>', `#include <begin_vertex>
    float ph = instanceMatrix[3].x*.9+instanceMatrix[3].z*.7+instanceMatrix[3].y*.8;
    transformed.y += sin(uT*1.6+ph)*.03;
    transformed += vec3(1.,0.,.4)*sin(uT*.6+instanceMatrix[3].x*.22)*.05;`);
}
function buildTrees(scene, spots) {
  const trees = [];
  for (let s = 2; s < LEN - 2; s += 6.5) for (const side of [-1, 1]) {
    const u = s / LEN, p = atPath(u, side * (SW + .75));
    if (!near(KIOSK, p.x, p.z, 3.4) && !near(VAULT, p.x, p.z, 3)) trees.push([p.x, p.z, .45 + rnd() * .15]);
  }
  for (let x = X0 + 3; x < X1 - 3; x += 6.5) trees.push([x + rnd(), seaZ(x) - ROAD - RW - .8, .42 + rnd() * .12]);
  for (const [x, z] of spots) if (rnd() < .35 && streetD(x, z) > SW + 3 && roadD(x, z) > RW + 1.4 && z < seaZ(x) - 3 && !near(KIOSK, x, z, 4) && !near(VAULT, x, z, 3.6)) trees.push([x, z, .7 + rnd() * .35]);
  const pal = [[.26, .5, .16], [.36, .58, .18], [.18, .4, .14], [.48, .62, .2], [.3, .46, .26]];
  const leaves = [], trunks = [];
  for (const [x, z, s] of trees) {
    const H0 = 3.4 * s, tg = new THREE.CylinderGeometry(.07 * s, .16 * s, H0, 6); tg.translate(x, H0 / 2, z); trunks.push(colored(tg, '#5a4436'));
    for (let l = 0; l < 7; l++) {
      const a = rnd() * 6.28, r = rnd() ** .6 * 1.9 * s, lx = x + Math.cos(a) * r, lz = z + Math.sin(a) * r, ly = H0 * .85 + (rnd() - .3) * 1.2 * s + (1 - r / (2 * s)) * s;
      const lr = (.7 + rnd() * .5) * s;
      for (let i = 0; i < 95; i++) {
        const uu = rnd() * 2 - 1, th = rnd() * 6.28, q = Math.sqrt(1 - uu * uu), rr = lr * Math.cbrt(rnd());
        const c = pal[(rnd() * pal.length) | 0], lit = .7 + .65 * clamp(uu * .5 + .5);
        leaves.push(lx + q * Math.cos(th) * rr, ly + uu * rr * .75, lz + q * Math.sin(th) * rr, c[0] * lit, c[1] * lit, c[2] * lit);
      }
    }
  }
  const tm = new THREE.Mesh(mergeGeometries(trunks), vcMat(CLS.wall)); tm.castShadow = true; scene.add(tm);
  const count = leaves.length / 6, im = new THREE.InstancedMesh(new THREE.PlaneGeometry(.26, .2), tag(new THREE.MeshLambertMaterial({ side: THREE.DoubleSide }), CLS.leaf, leafWind), count);
  const o = new THREE.Object3D(), c = new THREE.Color();
  for (let i = 0; i < count; i++) {
    o.position.set(leaves[i * 6], leaves[i * 6 + 1], leaves[i * 6 + 2]); o.rotation.set(rnd() * 6, rnd() * 6, rnd() * 6); o.updateMatrix(); im.setMatrixAt(i, o.matrix);
    im.setColorAt(i, c.setRGB(leaves[i * 6 + 3], leaves[i * 6 + 4], leaves[i * 6 + 5]));
  }
  im.castShadow = im.receiveShadow = true; scene.add(im);
}

// ---------- seafront: railing, lamps, tetrapods ----------
function buildSeafront(scene) {
  const o = new THREE.Object3D();
  const posts = [];
  for (let x = X0 + 2; x < X1 - 2; x += .7) posts.push(x);
  const pm = new THREE.InstancedMesh(new THREE.BoxGeometry(.08, .5, .08), tag(new THREE.MeshLambertMaterial({ color: '#c9ccd4' }), CLS.stone), posts.length);
  posts.forEach((x, i) => { o.position.set(x, .25, seaZ(x) - .25); o.updateMatrix(); pm.setMatrixAt(i, o.matrix); });
  pm.castShadow = true; scene.add(pm);
  const rail = []; for (let x = X0 + 2; x < X1 - 2; x += .7) { const z0 = seaZ(x) - .25, z1 = seaZ(x + .7) - .25, b = new THREE.BoxGeometry(.75, .06, .06); b.rotateY(-Math.atan2(z1 - z0, .7)); b.translate(x + .35, .5, (z0 + z1) / 2); rail.push(colored(b, '#d6d8de')); }
  scene.add(new THREE.Mesh(mergeGeometries(rail), vcMat(CLS.stone)));
  const lamps = []; for (let x = X0 + 3; x < X1 - 3; x += 3.3) lamps.push([x, seaZ(x) - .9]);
  for (let s = 3; s < LEN - 2; s += 5.5) { const p = atPath(s / LEN, -(SW + .4)); if (!near(VAULT, p.x, p.z, 2.5)) lamps.push([p.x, p.z]); }
  const lp = new THREE.InstancedMesh(new THREE.CylinderGeometry(.05, .07, 2.2, 6), tag(new THREE.MeshLambertMaterial({ color: '#3a3e4a' }), CLS.stone), lamps.length);
  const lh = new THREE.InstancedMesh(new THREE.SphereGeometry(.17, 8, 6), tag(new THREE.MeshBasicMaterial({ color: new THREE.Color(1.6, 1.1, .62) }), CLS.glow), lamps.length);
  lamps.forEach(([x, z], i) => { o.position.set(x, 1.1, z); o.updateMatrix(); lp.setMatrixAt(i, o.matrix); o.position.y = 2.25; o.updateMatrix(); lh.setMatrixAt(i, o.matrix); });
  lp.castShadow = true; scene.add(lp, lh);
  const rg = new THREE.IcosahedronGeometry(1, 1), p = rg.attributes.position;
  for (let i = 0; i < p.count; i++) { const v = new THREE.Vector3().fromBufferAttribute(p, i), k = .75 + fbm(v.x * 1.7 + 4, v.y * 1.7 + v.z) * .5; p.setXYZ(i, v.x * k, v.y * k * .7, v.z * k); }
  rg.computeVertexNormals();
  const N = 120, rm = new THREE.InstancedMesh(rg, tag(new THREE.MeshLambertMaterial({ color: '#8c93a3' }), CLS.stone), N);
  for (let i = 0; i < N; i++) { const x = X0 + 2 + rnd() * (X1 - X0 - 4), z = seaZ(x) + .3 + rnd() * 1.3, r = .25 + rnd() * .3; o.position.set(x, groundY(x, z) + r * .4, z); o.rotation.set(rnd() * 3, rnd() * 6, 0); o.scale.setScalar(r); o.updateMatrix(); rm.setMatrixAt(i, o.matrix); }
  rm.castShadow = rm.receiveShadow = true; scene.add(rm);
}

// ---------- the focal pair: recharge kiosk and shielded vault ----------
function place(g, p) { g.position.set(p.x, 0, p.z); g.rotation.y = Math.atan2(p.nx, p.nz); g.traverse(o => { o.castShadow = o.receiveShadow = true; }); return g; }
function buildKiosk(scene) {
  const g = new THREE.Group(), body = [], awn = [];
  const box = (w, h, d, x, y, z, c, arr) => { const b = new THREE.BoxGeometry(w, h, d); b.translate(x, y, z); arr.push(colored(b, c)); };
  box(2.6, 1.9, 1.6, 0, .95, -.1, '#2a49c0', body);           // cobalt shop body
  box(2.7, .12, 1.7, 0, 1.96, -.1, '#F0582A', awn);           // orange roof
  box(2.6, .9, .5, 0, .45, .95, '#e9dcc2', body);             // counter
  box(2.2, .45, .08, 0, 2.3, .62, '#F4EFE4', body);           // signboard
  for (let i = 0; i < 9; i++) { const b = new THREE.BoxGeometry(.3, .07, 1.25); b.rotateX(.42); b.translate(-1.2 + i * .3, 1.78, 1.25); awn.push(colored(b, i % 2 ? '#F4EFE4' : '#F0582A')); } // striped awning
  g.add(new THREE.Mesh(mergeGeometries(body), vcMat(CLS.wall)));
  g.add(new THREE.Mesh(mergeGeometries(awn), vcMat(CLS.kiosk, { emissive: '#7a2008', emissiveIntensity: .6, side: THREE.DoubleSide })));
  const scr = new THREE.Mesh(new THREE.PlaneGeometry(1.9, .7), tag(new THREE.MeshBasicMaterial({ color: new THREE.Color(.75, .95, 1.7) }), CLS.glow)); // phone-plan screen
  scr.position.set(0, 1.2, .71); g.add(scr);
  g.scale.setScalar(1.3); scene.add(place(g, KIOSK));
}
function buildVault(scene) {
  const g = new THREE.Group(), parts = [];
  const b = new THREE.BoxGeometry(1.7, 1.3, 1.5); b.translate(0, .65, 0); parts.push(colored(b, '#1E3FAE'));
  const p = new THREE.BoxGeometry(2.1, .18, 1.9); p.translate(0, .09, 0); parts.push(colored(p, '#c7c9d2'));
  g.add(new THREE.Mesh(mergeGeometries(parts), vcMat(CLS.vault, { emissive: '#0b1a55', emissiveIntensity: .7 })));
  const s = new THREE.Shape(); s.moveTo(0, 1); s.lineTo(.75, .78); s.quadraticCurveTo(.75, -.35, 0, -.85); s.quadraticCurveTo(-.75, -.35, -.75, .78); s.closePath();
  const sg = new THREE.ExtrudeGeometry(s, { depth: .22, bevelEnabled: false, curveSegments: 8 }); sg.scale(.8, .8, 1);
  const shield = new THREE.Mesh(sg, tag(new THREE.MeshBasicMaterial({ color: new THREE.Color(.62, .76, 1.5), side: THREE.DoubleSide }), CLS.vault));
  shield.position.set(0, 2.15, 0); shield.rotation.x = -.35; g.add(shield);
  scene.add(place(g, VAULT));
  return shield;
}
function personGeo() {
  const b = new THREE.CylinderGeometry(.15, .2, .72, 7); b.translate(0, .36, 0);
  const h = new THREE.SphereGeometry(.15, 8, 6); h.translate(0, .86, 0);
  return [b, h];
}

// ---------- ascii + tilt shift ----------
const GLYPHS = " .'`,:;-~=+*oOsS$%&8#@/\\|_^v\u2014";
const GI = Object.fromEntries([...GLYPHS].map((c, i) => [c, i]));
const gg = (c) => GI[c].toFixed(1);
function buildAtlas() {
  const tw = 64, th = 120, c = document.createElement('canvas');
  c.width = tw * GLYPHS.length; c.height = th;
  const x = c.getContext('2d');
  x.fillStyle = '#000'; x.fillRect(0, 0, c.width, c.height); x.fillStyle = '#fff'; x.textAlign = 'center'; x.textBaseline = 'middle';
  x.font = 'bold 100px "Geist Mono", Menlo, Monaco, monospace';
  [...GLYPHS].forEach((ch, i) => { if (ch === '\u2014') x.fillRect(i * tw, th * .44, tw, th * .12); else x.fillText(ch, (i + .5) * tw, th * .52); });
  const t = new THREE.CanvasTexture(c); t.generateMipmaps = true; t.minFilter = THREE.LinearMipmapLinearFilter; t.magFilter = THREE.LinearFilter; t.anisotropy = 8; t.flipY = false;
  return t;
}
const asciiFrag = /* glsl */`
precision highp float;
uniform sampler2D uScene, uAtlas; uniform vec2 uGrid; uniform float uTime, uN;
in vec2 vUv; out vec4 outColor;
float h21(vec2 p){ return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453); }
float vn(vec2 p){ vec2 i=floor(p),f=fract(p); f=f*f*(3.-2.*f); return mix(mix(h21(i),h21(i+vec2(1,0)),f.x),mix(h21(i+vec2(0,1)),h21(i+vec2(1,1)),f.x),f.y); }
float lum(vec3 c){ return dot(c,vec3(.25,.6,.15)); }
vec3 grade(vec3 c){
  c = max(c,0.); c = c/(1.+c*.12); c = pow(c,vec3(1./2.2));
  c = mix(c,c*c*(3.-2.*c),.5);
  float l = lum(c); c = mix(vec3(l),c,1.3);
  return clamp(c,0.,1.);
}
void main(){
  vec2 g = vUv*uGrid, cell = floor(g), cuv = (cell+.5)/uGrid;
  ivec2 sz = textureSize(uScene,0);
  float lodC = log2(float(sz.x)/uGrid.x)+.05;
  vec3 col = grade(textureLod(uScene,cuv,lodC).rgb);
  float cls = floor(texelFetch(uScene,ivec2(cuv*vec2(sz)),0).a*16.+.5);
  float L = lum(col);
  vec2 e = 1./uGrid;
  float lx0=lum(grade(textureLod(uScene,cuv-vec2(e.x,0),lodC).rgb)), lx1=lum(grade(textureLod(uScene,cuv+vec2(e.x,0),lodC).rgb));
  float ly0=lum(grade(textureLod(uScene,cuv-vec2(0,e.y),lodC).rgb)), ly1=lum(grade(textureLod(uScene,cuv+vec2(0,e.y),lodC).rgb));
  vec2 grad = vec2(lx1-lx0,(ly1-ly0)*.55); float mag = length(grad);
  float a4 = mod(atan(grad.y,grad.x)+.3927,3.14159)/.7854;
  float edgeG = a4<1.?${gg('|')}:a4<2.?${gg('\\')}:a4<3.?${gg('\u2014')}:${gg('/')};
  float w = vn(cell*vec2(.05,.09)+vec2(-uTime*.2,uTime*.05))*2.-1.;
  float gl = ${gg('.')};
  if(cls<1.5){        // asphalt
    gl = L<.08?${gg('.')}:L<.16?${gg('-')}:${gg('=')};
  } else if(cls<2.5){ // leaves
    gl = L<.1?${gg('.')}:L<.2?${gg(':')}:L<.32?${gg('*')}:L<.45?${gg('+')}:${gg('o')};
  } else if(cls<3.5){ // pavers
    gl = L<.14?${gg('.')}:L<.28?${gg(':')}:L<.48?${gg(';')}:${gg('+')};
  } else if(cls<4.5){ // stone, rails, posts
    gl = L<.18?${gg(':')}:L<.4?${gg('%')}:${gg('#')};
    if(mag>.16) gl = edgeG;
  } else if(cls<5.5){ // lane marking
    gl = ${gg('\u2014')};
  } else if(cls<6.5){ // kiosk awning
    gl = L<.1?${gg(':')}:L<.3?${gg('=')}:${gg('#')};
    if(mag>.2 && L>.08) gl = edgeG;
  } else if(cls<7.5){ // walls, trunks
    gl = L<.12?${gg(':')}:(mag>.12?edgeG:(L<.32?${gg('|')}:${gg('#')}));
  } else if(cls<8.5){ // rooftops
    gl = L<.14?${gg(':')}:L<.3?${gg('=')}:L<.5?${gg('%')}:${gg('#')};
    if(mag>.18) gl = edgeG;
  } else if(cls<9.5){ // water
    gl = L<.12?${gg('-')}:L<.3?(w>0.?${gg('~')}:${gg('-')}):L<.55?${gg('~')}:${gg('=')};
  } else if(cls<10.5){ // foam
    gl = L<.6?${gg('*')}:L<.82?${gg('%')}:${gg('#')};
  } else if(cls<11.5){ // people
    gl = L<.3?${gg('&')}:${gg('$')};
  } else if(cls<12.5){ // lit windows, lamps, screens
    gl = L<.45?${gg('o')}:L<.7?${gg('O')}:${gg('@')};
  } else if(cls<13.5){ // coins
    gl = L<.5?${gg('o')}:${gg('@')};
  } else if(cls<14.5){ // vault + shield
    gl = L<.15?${gg(':')}:L<.4?${gg('%')}:${gg('#')};
    if(mag>.22) gl = edgeG;
  } else {            // autos and cars
    gl = L<.25?${gg('&')}:${gg('8')};
  }
  vec2 gv = g-cell, dx = dFdx(g), dy = dFdy(g);
  float ink = smoothstep(.12,.78,textureGrad(uAtlas,vec2((gl+gv.x)/uN,1.-gv.y),dx/uN,dy).r);
  vec3 blur = grade(textureLod(uScene,vUv,lodC+1.2).rgb);
  outColor = vec4(mix(blur*.26,col*1.32+.012,ink),1.);
}`;
const tiltFrag = /* glsl */`
precision highp float;
uniform sampler2D uA; uniform vec2 uRes; uniform float uR;
in vec2 vUv; out vec4 outColor;
void main(){
  float dy = abs(vUv.y-.56);
  float r = pow(smoothstep(.25,.6,dy),1.6)*uR;
  if(r<.2){ outColor = vec4(texture(uA,vUv).rgb,1.); return; }
  vec3 c = vec3(0.);
  float lod = max(log2(r/4.),0.);
  for(int i=0;i<36;i++){
    float t = float(i)+.5, a = t*2.39996, rr = sqrt(t/36.)*r;
    c += textureLod(uA,vUv+vec2(cos(a),sin(a))*rr/uRes,lod).rgb;
  }
  outColor = vec4(c/36.,1.);
}`;

export async function start(host) {
  const renderer = new THREE.WebGLRenderer({ antialias: false, powerPreference: 'high-performance' });
  renderer.setPixelRatio(1);
  renderer.shadowMap.enabled = true; renderer.shadowMap.type = THREE.PCFShadowMap;
  const canvas = renderer.domElement;

  const scene = new THREE.Scene();
  const sun = new THREE.DirectionalLight('#ffb27a', 3.8); sun.position.set(-34, 30, 8); sun.target.position.set(0, 0, 0);
  sun.castShadow = true; sun.shadow.mapSize.set(2048, 2048);
  Object.assign(sun.shadow.camera, { left: -26, right: 26, top: 26, bottom: -26, near: 5, far: 110 });
  sun.shadow.bias = -.0006; sun.shadow.normalBias = .03;
  scene.add(sun, sun.target, new THREE.HemisphereLight('#a9b8f0', '#7a5c44', 1.9));

  scene.add(buildGround(), buildSea());
  const spots = buildBlocks(scene);
  buildTrees(scene, spots); buildSeafront(scene); buildKiosk(scene);
  const shield = buildVault(scene);

  // people: three at the kiosk with glowing phones, walkers on the pavements and promenade
  const [pb, ph] = personGeo();
  const shirts = ['#d9cbb0', '#2c4fd0', '#8a3d2c', '#e8e2d6', '#5c6b3a', '#b9a07a', '#3e3a52', '#c4644a'];
  const KN = 3, WN = 14, PN = KN + WN;
  const bodies = new THREE.InstancedMesh(pb, tag(new THREE.MeshLambertMaterial(), CLS.person), PN);
  const heads = new THREE.InstancedMesh(ph, tag(new THREE.MeshLambertMaterial({ color: '#7a5240' }), CLS.person), PN);
  const phones = new THREE.InstancedMesh(new THREE.BoxGeometry(.1, .16, .03), tag(new THREE.MeshBasicMaterial({ color: new THREE.Color(.8, 1, 1.8) }), CLS.glow), KN);
  for (let i = 0; i < PN; i++) bodies.setColorAt(i, new THREE.Color(shirts[i % shirts.length]));
  bodies.castShadow = heads.castShadow = true; scene.add(bodies, heads, phones);
  const walkers = Array.from({ length: WN }, (_, i) => ({ kind: i < 8 ? 'street' : 'prom', side: i % 2 ? 1 : -1, ph: rnd(), v: .55 + rnd() * .35, lane: rnd() }));

  // traffic on the seafront road: autos (black + yellow hood) and a couple of cars
  const ab = new THREE.BoxGeometry(1.5, .55, .8); ab.translate(0, .38, 0);
  const at = new THREE.BoxGeometry(1, .14, .82); at.translate(-.1, .95, 0);
  const autoG = mergeGeometries([colored(ab, '#3a3a40'), colored(at, '#ffffff')]); autoG.computeVertexNormals();
  const cars = new THREE.InstancedMesh(autoG, vcMat(CLS.auto), 6);
  ['#c8a82c', '#c8a82c', '#c8a82c', '#e6e2d8', '#7d8494', '#c8a82c'].forEach((h, i) => cars.setColorAt(i, new THREE.Color(h)));
  cars.castShadow = true; scene.add(cars);

  // coins: drift out of the vault, arc over the street and land at the kiosk counter
  const CN = 16, coins = new THREE.InstancedMesh(new THREE.CylinderGeometry(.2, .2, .05, 14), tag(new THREE.MeshBasicMaterial({ color: new THREE.Color(1.9, 1.15, .5) }), CLS.coin), CN);
  scene.add(coins);
  const A = new THREE.Vector3(VAULT.x, 2.6, VAULT.z), B = new THREE.Vector3(KIOSK.x + KIOSK.nx * 1.6, 1.6, KIOSK.z + KIOSK.nz * 1.6);

  const camera = new THREE.PerspectiveCamera(23, 1, 5, 400);
  const TARGET = new THREE.Vector3((KIOSK.x + VAULT.x) / 2, .8, (KIOSK.z + VAULT.z) / 2 + 4.6);
  function placeCamera(t) {
    const k = .5 - .5 * Math.cos(Math.PI * t / DURATION), az = THREE.MathUtils.degToRad(-7 + 14 * k), el = THREE.MathUtils.degToRad(54 + 1.4 * Math.sin(k * 3.14)), R = 74 - 2.5 * k;
    camera.position.set(TARGET.x + Math.sin(az) * Math.cos(el) * R, TARGET.y + Math.sin(el) * R, TARGET.z + Math.cos(az) * Math.cos(el) * R);
    camera.lookAt(TARGET); camera.updateMatrixWorld();
  }

  const rt = new THREE.WebGLRenderTarget(1, 1, { type: THREE.HalfFloatType, generateMipmaps: true, minFilter: THREE.LinearMipmapLinearFilter, magFilter: THREE.LinearFilter });
  const art = new THREE.WebGLRenderTarget(1, 1, { generateMipmaps: true, minFilter: THREE.LinearMipmapLinearFilter, magFilter: THREE.LinearFilter, depthBuffer: false });
  const vtx = `in vec3 position; in vec2 uv; out vec2 vUv; void main(){ vUv = uv; gl_Position = vec4(position.xy,0.,1.); }`;
  await document.fonts?.load('bold 100px "Geist Mono"').catch(() => {});
  const grid = new THREE.Vector2(), res = new THREE.Vector2(), blurR = { value: 26 };
  const ascii = new THREE.RawShaderMaterial({ glslVersion: THREE.GLSL3, vertexShader: vtx, fragmentShader: asciiFrag, depthTest: false, depthWrite: false,
    uniforms: { uScene: { value: rt.texture }, uAtlas: { value: buildAtlas() }, uGrid: { value: grid }, uTime: time, uN: { value: GLYPHS.length } } });
  const tilt = new THREE.RawShaderMaterial({ glslVersion: THREE.GLSL3, vertexShader: vtx, fragmentShader: tiltFrag, depthTest: false, depthWrite: false,
    uniforms: { uA: { value: art.texture }, uRes: { value: res }, uR: blurR } });
  const quad = new THREE.Mesh(new THREE.PlaneGeometry(2, 2), ascii); quad.frustumCulled = false;
  const post = new THREE.Scene(); post.add(quad); const postCam = new THREE.Camera();

  let cols = 0, rows = 0;
  function resize() {
    const w = host.clientWidth || 520, h = host.clientHeight || 650;
    const pw = Math.min(1200, Math.round(w * Math.min(devicePixelRatio || 1, 2))), phh = Math.round(pw * h / w);
    renderer.setSize(pw, phh, false); rt.setSize(pw, phh); art.setSize(pw, phh); res.set(pw, phh);
    cols = Math.round(w / 6.4); rows = Math.round(h / (w / cols / .53)); grid.set(cols, rows);
    blurR.value = 26 * phh / 1440 * 1.3;
    camera.aspect = w / h; camera.updateProjectionMatrix();
  }
  resize();

  const o = new THREE.Object3D(), tmp = new THREE.Vector3();
  function drawFrame(t) {
    time.value = t; placeCamera(t);
    for (let i = 0; i < PN; i++) {
      let x, z, ry, bob = 0, sc = 1;
      if (i < KN) { // queue at the counter, idle sway
        const off = (i - 1) * .75; x = KIOSK.x + KIOSK.nx * 2.4 + KIOSK.nz * off; z = KIOSK.z + KIOSK.nz * 2.4 - KIOSK.nx * off;
        ry = Math.atan2(-KIOSK.nx, -KIOSK.nz) + Math.sin(t * .7 + i * 2) * .25;
        o.position.set(x + Math.sin(ry) * .22, .78 + Math.sin(t * 1.3 + i) * .02, z + Math.cos(ry) * .22); o.rotation.set(-.6, ry, 0); o.scale.setScalar(1); o.updateMatrix(); phones.setMatrixAt(i, o.matrix);
      } else {
        const wk = walkers[i - KN], step = t * wk.v * 1.4;
        if (wk.kind === 'street') {
          const s = fract(wk.ph + step / LEN * wk.side), u = .03 + s * .94, p = atPath(u, wk.side * (SW + .45 + wk.lane * .5)), tg = curve.getTangentAt(u);
          x = p.x; z = p.z; ry = Math.atan2(tg.x * wk.side, tg.z * wk.side); sc = sstep(0, .04, s) * sstep(1, .96, s);
        } else {
          x = X0 + 6 + fract(wk.ph + step / 60 * wk.side) * 68; z = seaZ(x) - 1.1 - wk.lane * 1.1; ry = wk.side > 0 ? Math.PI / 2 : -Math.PI / 2;
        }
        bob = Math.abs(Math.sin(t * 7 * wk.v + wk.ph * 9)) * .07;
      }
      o.position.set(x, bob, z); o.rotation.set(0, ry, i < KN ? 0 : Math.sin(t * 7 * (walkers[i - KN]?.v || 0)) * .04); o.scale.setScalar(sc); o.updateMatrix();
      bodies.setMatrixAt(i, o.matrix); heads.setMatrixAt(i, o.matrix);
    }
    bodies.instanceMatrix.needsUpdate = heads.instanceMatrix.needsUpdate = phones.instanceMatrix.needsUpdate = true;
    for (let i = 0; i < 6; i++) {
      const dir = i % 2 ? 1 : -1, x = X0 + 8 + fract(i * .37 + t * (.05 + i * .006) * dir) * 64, z = seaZ(x) - ROAD + dir * .6;
      o.position.set(x, 0, z); o.rotation.set(0, dir > 0 ? 0 : Math.PI, 0); o.scale.setScalar(i === 3 || i === 4 ? 1.25 : 1); o.updateMatrix(); cars.setMatrixAt(i, o.matrix);
    }
    cars.instanceMatrix.needsUpdate = true;
    for (let i = 0; i < CN; i++) {
      const k = fract(t * .11 + i / CN);
      tmp.lerpVectors(A, B, k); tmp.y += Math.sin(Math.PI * k) * 3.2 + Math.sin(t * 2 + i) * .08;
      o.position.copy(tmp); o.rotation.set(Math.PI / 2 + Math.sin(t + i) * .4, t * 2.2 + i, 0); o.scale.setScalar(sstep(0, .08, k) * sstep(1, .9, k)); o.updateMatrix(); coins.setMatrixAt(i, o.matrix);
    }
    coins.instanceMatrix.needsUpdate = true;
    shield.position.y = 2.15 + Math.sin(t * 1.2) * .08;
    renderer.setRenderTarget(rt); renderer.setClearColor(0x000000, 0); renderer.clear(); renderer.render(scene, camera);
    quad.material = ascii; renderer.setRenderTarget(art); renderer.render(post, postCam);
    quad.material = tilt; renderer.setRenderTarget(null); renderer.render(post, postCam);
  }

  let running = false, raf = 0, clock = 0, last = 0;
  const loop = (now) => { clock += Math.min(.1, (now - last) / 1000); last = now; drawFrame(clock); if (running) raf = requestAnimationFrame(loop); };
  drawFrame(0);
  return {
    canvas,
    get cols() { return cols; }, get rows() { return rows; },
    play(on) { if (on === running) return; running = on; if (on) { last = performance.now(); raf = requestAnimationFrame(loop); } else cancelAnimationFrame(raf); },
    resize() { resize(); drawFrame(clock); },
  };
}
