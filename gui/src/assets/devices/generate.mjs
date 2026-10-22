#!/usr/bin/env node
// Builds the device line drawings (*.svg), their label layouts (layouts/<device>.ts and the
// index) and preview.html.
//
// Every shape comes from our own measurements of the devices, in millimetres:
// x grows to the right of the centre line, y grows from the front (top) edge towards
// the player. Each device is scaled into a 1000×700 viewBox. The generated files are
// outputs; change this script and run, from gui/:
//
//   node src/assets/devices/generate.mjs
//
// The script checks its own output (well-formed XML, control names, leader crossings,
// chip overlaps) and exits non-zero on problems. preview.html runs the shape-based
// checks (leaders over other controls, chips over the drawing, overlapping hit areas).

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const OUT = dirname(fileURLToPath(import.meta.url));
const VIEWBOX = [0, 0, 1000, 700];
/** Largest label chip the layouts leave room for (viewBox units); the controller view caps
 * its chips at this width (`.chip { max-width: 15% }` of the 1000 wide viewBox). */
const CHIP = { w: 150, h: 28 };
/** Front drawings of controllers with motion sensors, whose gyro can be mapped. */
const MOTION = new Set(['dualsense', 'dualsenseEdge', 'switchpro', 'switch2pro', 'joycon', 'joycon2']);

// Front views show the bumpers but not the triggers behind them (the GameCube's big L / R
// triggers excepted).
const PAD = ['A', 'B', 'X', 'Y', 'LB', 'RB', 'Back', 'Start', 'Guide', 'Misc', 'LS', 'RS'];
const DPAD = ['DpadUp', 'DpadDown', 'DpadLeft', 'DpadRight'];
const SHOULDERS = ['LB', 'RB', 'LT', 'RT'];
const PADDLES = ['PaddleUL', 'PaddleUR', 'PaddleLL', 'PaddleLR'];
// Every drawing shows exactly the controls its device has, as far as they show from that
// side. Back views show the triggers, the bumpers where they show from behind, and the
// back buttons the model has.
const EXPECTED = {
  dualsense: [...PAD, ...DPAD, 'Touchpad'],
  dualsenseEdge: [...PAD, ...DPAD, 'Touchpad', 'PaddleUL', 'PaddleUR'],
  xbox: [...PAD, ...DPAD],
  generic: [...PAD, ...DPAD],
  switchpro: [...PAD, ...DPAD],
  switch2pro: [...PAD, ...DPAD, 'Misc2'],
  joycon: [...PAD, ...DPAD, ...PADDLES],
  joycon2: [...PAD, ...DPAD, 'Misc2', ...PADDLES],
  gamecube: [...PAD.filter((c) => c !== 'Back'), 'LT', 'RT', ...DPAD, 'Misc2'],
  dualsenseBack: ['LT', 'RT'],
  dualsenseEdgeBack: ['LT', 'RT', 'PaddleLL', 'PaddleLR'],
  xboxBack: SHOULDERS,
  xboxEliteBack: [...SHOULDERS, ...PADDLES],
  genericBack: [...SHOULDERS, ...PADDLES],
  switchproBack: SHOULDERS,
  switch2proBack: [...SHOULDERS, 'PaddleUL', 'PaddleUR'],
  joyconBack: [...SHOULDERS, ...PADDLES],
  gamecubeBack: SHOULDERS,
  mouse: ['Mouse:Left', 'Mouse:Right', 'Mouse:Middle', 'Mouse:X1', 'Mouse:X2', 'Wheel:Up', 'Wheel:Down'],
};

// ---------------------------------------------------------------- geometry (mm)

const r1 = (n) => Math.round(n * 10) / 10 + 0; // `+ 0` turns -0 into 0
const num = (n) => String(r1(n));
const rad = (deg) => (deg * Math.PI) / 180;
/** Rotate around the origin; positive angles turn clockwise on screen (y points down). */
const rot = ([x, y, ...rest], deg) => [x * Math.cos(rad(deg)) - y * Math.sin(rad(deg)), x * Math.sin(rad(deg)) + y * Math.cos(rad(deg)), ...rest];
const move = ([x, y, ...rest], [dx, dy]) => [x + dx, y + dy, ...rest];
const mirror = (pts) => pts.map(([x, y, ...rest]) => [-x, y, ...rest]);
/** Mirror to the right-hand side when s is -1 (shapes are written for the left side). */
const flip = (pts, s) => (s < 0 ? mirror(pts) : pts);
/** Closed outline from its left half, which runs from the top centre to the bottom centre. */
const fromHalf = (half) => [...half, ...mirror(half.slice(1, -1)).reverse()];
const onCircle = ([cx, cy], r, deg) => [cx + r * Math.cos(rad(deg)), cy + r * Math.sin(rad(deg))];

/** Both edges of a band of half-width hw around a centre line. */
function bandSides(pts, hw) {
  const n = pts.length;
  const nrm = pts.map((_, i) => {
    const a = pts[Math.max(0, i - 1)];
    const b = pts[Math.min(n - 1, i + 1)];
    const l = Math.hypot(b[0] - a[0], b[1] - a[1]);
    return [-(b[1] - a[1]) / l, (b[0] - a[0]) / l];
  });
  return {
    a: pts.map((p, i) => [p[0] + nrm[i][0] * hw, p[1] + nrm[i][1] * hw]),
    b: pts.map((p, i) => [p[0] - nrm[i][0] * hw, p[1] - nrm[i][1] * hw]),
  };
}
/** Outermost point of a band's rounded end (end 0 = first point, -1 = last). */
function bandTip(pts, hw, end) {
  const [i, j] = end === 0 ? [0, 1] : [pts.length - 1, pts.length - 2];
  const dx = pts[i][0] - pts[j][0];
  const dy = pts[i][1] - pts[j][1];
  const l = Math.hypot(dx, dy);
  return [pts[i][0] + (dx / l) * hw, pts[i][1] + (dy / l) * hw];
}
/** Point on a band's edge beside centre point i (side +1 / -1 picks the edge). */
const bandEdge = (pts, hw, i, side) => bandSides(pts, hw)[side > 0 ? 'a' : 'b'][i];

/** Cubic Bézier segments [start, c1, c2, end] of the Catmull-Rom spline through the points. */
function splineSegs(pts, closed = false) {
  const n = pts.length;
  const at = (i) => (closed ? pts[(i + n) % n] : pts[Math.max(0, Math.min(n - 1, i))]);
  const segs = [];
  for (let i = 0; i < (closed ? n : n - 1); i++) {
    const [p0, p1, p2, p3] = [at(i - 1), at(i), at(i + 1), at(i + 2)];
    const c1 = p1[2] ? p1 : [p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6];
    const c2 = p2[2] ? p2 : [p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6];
    segs.push([p1, c1, c2, p2]);
  }
  return segs;
}
const bezier = ([a, b, c, e], t) => {
  const u = 1 - t;
  return [0, 1].map((k) => u * u * u * a[k] + 3 * u * u * t * b[k] + 3 * u * t * t * c[k] + t * t * t * e[k]);
};
/** Points along Bézier segments, `per` per segment, the last end included. */
const along = (segs, per) => [...segs.flatMap((sg) => Array.from({ length: per }, (_, i) => bezier(sg, i / per))), segs.at(-1)[3].slice(0, 2)];
/** y of a half outline's top edge at x, as the drawn outline runs. */
function topY(half, x) {
  const pts = along(splineSegs(fromHalf(half), true), 40);
  for (let i = 1; i < pts.length; i++) {
    const [a, b] = [pts[i - 1], pts[i]];
    if (a[0] >= x && b[0] <= x) return a[1] + ((b[1] - a[1]) * (x - a[0])) / (b[0] - a[0] || 1);
  }
  throw new Error(`no top edge at x = ${x}`);
}
/** Index of the point at x in a half outline (shoulder ends are given as outline points). */
const pointAt = (half, x) => {
  const i = half.findIndex((q) => q[0] === x);
  if (i < 0) throw new Error(`no outline point at x = ${x}`);
  return i;
};
/** Points pushed out from their centroid by mm (hit areas of plate-shaped controls). */
function grow(pts, mm) {
  const [cx, cy] = [0, 1].map((k) => pts.reduce((s, q) => s + q[k], 0) / pts.length);
  return pts.map(([x, y, ...rest]) => {
    const l = Math.hypot(x - cx, y - cy) || 1;
    return [x + ((x - cx) / l) * mm, y + ((y - cy) / l) * mm, ...rest];
  });
}

// ---------------------------------------------------------------- device builder

class Device {
  constructor(id, { title, file, scale, ox, oy, colL = 200, colR = 800 }) {
    Object.assign(this, { id, title, file, s: scale, ox, oy, colL, colR });
    this.body = [];
    this.details = [];
    this.controls = [];
    this.labels = [];
  }

  pt([x, y]) {
    return [r1(this.ox + x * this.s), r1(this.oy + y * this.s)];
  }
  p(q) {
    return this.pt(q).join(' ');
  }
  len(mm) {
    return num(mm * this.s);
  }

  // Path data; all input in mm.

  /** Catmull-Rom spline through the points; a truthy third value makes a corner. */
  spline(pts, closed = false) {
    return `M${this.p(pts[0])}${this.curve(splineSegs(pts, closed))}${closed ? 'Z' : ''}`;
  }
  /** Path data of Bézier segments from splineSegs, without the leading move. */
  curve(segs) {
    return segs.map(([, c1, c2, q]) => `C${this.p(c1)} ${this.p(c2)} ${this.p(q)}`).join('');
  }
  /** Polygon (or open polyline) whose corners are rounded by the third value. */
  poly(pts, closed = true) {
    const n = pts.length;
    let d = '';
    pts.forEach(([x, y, r = 0], i) => {
      const cmd = i ? 'L' : 'M';
      if (!r || (!closed && (i === 0 || i === n - 1))) {
        d += cmd + this.p([x, y]);
        return;
      }
      const toward = ([qx, qy]) => {
        const l = Math.hypot(qx - x, qy - y);
        return [x + ((qx - x) / l) * r, y + ((qy - y) / l) * r];
      };
      d += `${cmd}${this.p(toward(pts[(i - 1 + n) % n]))}Q${this.p([x, y])} ${this.p(toward(pts[(i + 1) % n]))}`;
    });
    return closed ? `${d}Z` : d;
  }
  /** Stadium shape around the segment a–b. */
  capsule(a, b, r) {
    const l = Math.hypot(b[0] - a[0], b[1] - a[1]);
    const [nx, ny] = [(-(b[1] - a[1]) / l) * r, ((b[0] - a[0]) / l) * r];
    const R = this.len(r);
    return (
      `M${this.p([a[0] + nx, a[1] + ny])}L${this.p([b[0] + nx, b[1] + ny])}` +
      `A${R} ${R} 0 0 0 ${this.p([b[0] - nx, b[1] - ny])}L${this.p([a[0] - nx, a[1] - ny])}` +
      `A${R} ${R} 0 0 0 ${this.p([a[0] + nx, a[1] + ny])}Z`
    );
  }
  /** Pie slice from angle a0 to a1 (degrees, clockwise from +x). */
  wedge(c, r, a0, a1) {
    const R = this.len(r);
    return `M${this.p(c)}L${this.p(onCircle(c, r, a0))}A${R} ${R} 0 0 1 ${this.p(onCircle(c, r, a1))}Z`;
  }
  /** Band of half-width hw with round ends along a smooth centre line. */
  band(pts, hw) {
    const { a, b } = bandSides(pts, hw);
    const back = b.slice().reverse();
    const R = this.len(hw);
    return (
      this.spline(a) +
      `A${R} ${R} 0 0 0 ${this.p(back[0])}` +
      this.spline(back).replace(/^M[^C]*/, '') +
      `A${R} ${R} 0 0 0 ${this.p(a[0])}Z`
    );
  }
  /** Path data from command letters and mm points: D('M', p, 'L', q). */
  D(...parts) {
    return parts
      .map((q) => (typeof q === 'string' ? q : `${this.p(q)} `))
      .join('')
      .replace(/ ([A-Za-z])/g, '$1')
      .trim();
  }
  /** Face-button glyphs and letters, h = glyph height. */
  glyph(kind, [cx, cy], h) {
    const P = (dx, dy) => this.p([cx + dx * h, cy + dy * h]);
    const R = (f) => this.len(f * h);
    switch (kind) {
      case 'triangle':
        return `M${P(0, -0.55)}L${P(0.476, 0.275)}L${P(-0.476, 0.275)}Z`;
      case 'square':
        return `M${P(-0.4, -0.4)}L${P(0.4, -0.4)}L${P(0.4, 0.4)}L${P(-0.4, 0.4)}Z`;
      case 'cross':
        return `M${P(-0.42, -0.42)}L${P(0.42, 0.42)}M${P(0.42, -0.42)}L${P(-0.42, 0.42)}`;
      case 'circle':
        return `M${P(-0.46, 0)}A${R(0.46)} ${R(0.46)} 0 1 0 ${P(0.46, 0)}A${R(0.46)} ${R(0.46)} 0 1 0 ${P(-0.46, 0)}Z`;
      case 'A':
        return `M${P(-0.36, 0.5)}L${P(0, -0.5)}L${P(0.36, 0.5)}M${P(-0.21, 0.14)}L${P(0.21, 0.14)}`;
      case 'B':
        return (
          `M${P(-0.28, -0.5)}L${P(-0.28, 0.5)}L${P(0.06, 0.5)}A${R(0.25)} ${R(0.25)} 0 0 0 ${P(0.06, 0)}L${P(-0.28, 0)}` +
          `M${P(0.02, 0)}A${R(0.25)} ${R(0.25)} 0 0 0 ${P(0.02, -0.5)}L${P(-0.28, -0.5)}`
        );
      case 'X':
        return `M${P(-0.32, -0.5)}L${P(0.32, 0.5)}M${P(0.32, -0.5)}L${P(-0.32, 0.5)}`;
      case 'Y':
        return `M${P(-0.34, -0.5)}L${P(0, 0.02)}L${P(0.34, -0.5)}M${P(0, 0.02)}L${P(0, 0.5)}`;
      case 'C':
        return `M${P(0.33, -0.38)}A${R(0.5)} ${R(0.5)} 0 1 0 ${P(0.33, 0.38)}`;
      case 'minus':
        return `M${P(-0.42, 0)}L${P(0.42, 0)}`;
      case 'plus':
        return `M${P(-0.42, 0)}L${P(0.42, 0)}M${P(0, -0.42)}L${P(0, 0.42)}`;
      case 'home':
        return `M${P(-0.36, 0.4)}L${P(-0.36, -0.06)}L${P(0, -0.4)}L${P(0.36, -0.06)}L${P(0.36, 0.4)}Z`;
      case 'arrow-up':
      case 'arrow-right':
      case 'arrow-down':
      case 'arrow-left': {
        const deg = { 'arrow-up': 0, 'arrow-right': 90, 'arrow-down': 180, 'arrow-left': 270 }[kind];
        const [a, b, c] = [[0, -0.42], [0.4, 0.26], [-0.4, 0.26]].map((q) => rot(q, deg));
        return `M${P(...a)}L${P(...b)}L${P(...c)}Z`;
      }
      default:
        throw new Error(`unknown glyph ${kind}`);
    }
  }

  // Elements.

  path(d, extra = '') {
    return `<path${extra && ` ${extra}`} d="${d}"/>`;
  }
  circle(c, r, extra = '') {
    const [x, y] = this.pt(c);
    return `<circle${extra && ` ${extra}`} cx="${x}" cy="${y}" r="${this.len(r)}"/>`;
  }
  hit(d) {
    return this.path(d, 'class="hit" fill="currentColor" fill-opacity="0" stroke="none"');
  }
  hitCircle(c, r) {
    return this.circle(c, r, 'class="hit" fill="currentColor" fill-opacity="0" stroke="none"');
  }
  #el(x) {
    return x.startsWith('<') ? x : this.path(x);
  }
  outline(pts) {
    this.body.push(this.path(this.spline(pts, true), 'class="outline"'));
  }
  line(x) {
    this.body.push(this.#el(x));
  }
  detail(x) {
    this.details.push(this.#el(x));
  }
  /** A clickable control: its hit area first, then the visible parts (markup or path data). */
  control(name, hit, parts, attrs = '') {
    this.controls.push({ name, attrs, hit, parts: parts.map((x) => this.#el(x)) });
  }
  /**
   * Label placement. `at` is a y (mm) in the left/right label column, or an [x, y] (mm)
   * point. `via` bends the leader; `allow` lists controls the leader may cross (checks only).
   */
  label(control, anchor, side, at, { via = [], allow = [] } = {}) {
    const labelAt = typeof at === 'number' ? [side === 'left' ? this.colL : this.colR, this.pt([0, at])[1]] : this.pt(at);
    this.labels.push({ control, anchor: this.pt(anchor), side, labelAt, via: via.map((q) => this.pt(q)), allow });
  }

  svg() {
    const pad = (n) => (s) => ' '.repeat(n) + s;
    const out = [
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${VIEWBOX.join(' ')}" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" data-device="${this.id}">`,
      `  <!-- ${this.title}. Doppelpad line art, generated by generate.mjs from our own measurements. -->`,
      '  <g class="body" pointer-events="none">',
      ...this.body.map(pad(4)),
    ];
    if (this.details.length) out.push('    <g class="detail" stroke-width="1.75" opacity="0.45">', ...this.details.map(pad(6)), '    </g>');
    out.push('  </g>');
    for (const c of this.controls) {
      out.push(`  <g data-control="${c.name}"${c.attrs && ` ${c.attrs}`}>`, `    ${c.hit}`, ...c.parts.map(pad(4)), '  </g>');
    }
    out.push('</svg>', '');
    return out.join('\n');
  }

  layoutTs() {
    const pt = ([x, y]) => `[${x}, ${y}]`;
    const rows = this.labels.map(
      (l) =>
        `    { control: '${l.control}', anchor: ${pt(l.anchor)}, side: '${l.side}', labelAt: ${pt(l.labelAt)}` +
        `${l.via.length ? `, via: [${l.via.map(pt).join(', ')}]` : ''} },`,
    );
    return [
      `// Label layout for ${this.file}. Generated by ../generate.mjs; do not edit by hand.`,
      '',
      `import svg from '../${this.file}?raw';`,
      "import type { DeviceLayout } from './types';",
      '',
      `export const ${this.id}: DeviceLayout = {`,
      `  id: '${this.id}',`,
      '  svg,',
      `  viewBox: [${VIEWBOX.join(', ')}],`,
      ...(MOTION.has(this.id) ? ['  motion: true,'] : []),
      '  controls: [',
      ...rows,
      '  ],',
      '};',
      '',
    ].join('\n');
  }
}

// ---------------------------------------------------------------- shared parts

/** Analog stick: the opening in the shell, the cap and the cap's dished top. */
function stick(d, name, side, c, well, cap) {
  d.control(
    name,
    d.hitCircle(c, well + 1),
    [d.circle(c, well), `<g class="stick-cap">${d.circle(c, cap)}${d.circle(c, cap * 0.74, 'stroke-width="1.75" opacity="0.5"')}</g>`],
    `data-stick="${side}"`,
  );
}

/**
 * Face buttons around c; glyphs per button in Y, B, A, X order (top, right, bottom, left).
 * `offset` is the distance from c, or [sideways, up/down] for a diamond that is wider
 * than tall.
 */
function faceButtons(d, c, offset, r, glyphs, glyphH, hitR) {
  const [ox, oy] = typeof offset === 'number' ? [offset, offset] : offset;
  const dirs = [['Y', [0, -1]], ['B', [1, 0]], ['A', [0, 1]], ['X', [-1, 0]]];
  const centers = {};
  dirs.forEach(([name, [dx, dy]], i) => {
    const bc = [c[0] + dx * ox, c[1] + dy * oy];
    centers[name] = bc;
    d.control(name, d.hitCircle(bc, hitR), [d.circle(bc, r), d.path(d.glyph(glyphs[i], bc, glyphH), 'stroke-width="2.5"')]);
  });
  return centers;
}

/** Outline of a plus-shaped part: arms of half-width hw reaching len from c. */
function crossShape([cx, cy], hw, len, r) {
  const quarter = [[-hw, -len, r], [hw, -len, r], [hw, -hw, r * 0.6]];
  return [0, 90, 180, 270].flatMap((deg) => quarter.map(([x, y, rr]) => [...rot([x, y], deg), rr])).map(([x, y, rr]) => [cx + x, cy + y, rr]);
}

/**
 * One-piece cross d-pad drawn arm by arm, so each direction lights up on its own.
 * `arrows` adds a small arrow to each arm, `recess` the well the cross sits in.
 */
function crossDpad(d, c, hw, len, hitR, { arrows = false, recess = 0 } = {}) {
  if (recess) d.detail(d.poly(crossShape(c, hw + recess, len + recess, 2.4)));
  const arm = [[-hw, -hw], [-hw, -len, 1.7], [hw, -len, 1.7], [hw, -hw]];
  for (const [name, dir, deg] of [['DpadUp', 'up', 0], ['DpadRight', 'right', 90], ['DpadDown', 'down', 180], ['DpadLeft', 'left', 270]]) {
    const pts = arm.map((q) => move(rot(q, deg), c));
    const parts = [d.poly(pts, false)];
    if (arrows) parts.push(d.path(d.glyph(`arrow-${dir}`, move(rot([0, -len * 0.64], deg), c), hw * 0.5), 'stroke-width="1.75"'));
    d.control(name, d.hit(d.wedge(c, hitR, deg - 135, deg - 45)), parts);
  }
}

/**
 * Simplified back view of the shoulders, seen through from the front: each bumper is a band
 * on the top edge (as in the front view), each trigger a rounded plate on the back below it.
 * Shapes are given for the left side. Labels go to rows `bumperRow` and `triggerRow`.
 */
function backShoulders(d, { bumper, bumperHw, trigger, bumperRow = -8, triggerRow = 12 }) {
  for (const s of [1, -1]) {
    const side = s > 0 ? 'left' : 'right';
    const [lb, lt] = s > 0 ? ['LB', 'LT'] : ['RB', 'RT'];
    const b = flip(bumper, s);
    d.control(lb, d.hit(d.band(b, bumperHw + 1)), [d.band(b, bumperHw)]);
    const t = flip(trigger, s);
    const plate = d.poly(t);
    // two grip ridges across the plate
    const [ti, to, bo, bi] = t;
    const lerp = (p, q, f) => [p[0] + (q[0] - p[0]) * f, p[1] + (q[1] - p[1]) * f];
    const ridges = [0.42, 0.62].map((f) => {
      const [a, e] = [lerp(ti, bi, f), lerp(to, bo, f)];
      return d.D('M', lerp(a, e, 0.22), 'L', lerp(a, e, 0.78));
    });
    d.control(lt, d.hit(plate), [plate, d.path(ridges.join(''), 'stroke-width="1.75" opacity="0.6"')]);
    d.label(lb, bandEdge(b, bumperHw, Math.floor(b.length / 2), s), side, bumperRow);
    // anchor: on the plate's outer edge, low enough to pass under the bumper's end
    const at = lerp(to, bo, 0.6);
    d.label(lt, [at[0] + 0.3 * s, at[1]], side, triggerRow);
  }
}

/** USB port on the top edge, as seen from behind. */
function usbPort(d, y) {
  d.detail(d.poly([[-4.6, y - 1.2, 1.1], [4.6, y - 1.2, 1.1], [4.6, y + 1.2, 1.1], [-4.6, y + 1.2, 1.1]]));
}

/**
 * A shoulder button that forms part of the silhouette, on both sides. Its top edge is the
 * outline's own run from half-outline point i0 (inner end) to i1 (outer end), so both draw
 * the same curve; `lower` is its lower edge from the outer end back to the inner end, for
 * the left side. `parts(s)` adds markup inside the button; `lowerPad` (mm) widens the hit
 * area below the button, where nothing sits close under it.
 */
function seatedShoulder(d, half, i0, i1, lower, [left, right], { parts = () => [], lowerPad = 0.7 } = {}) {
  const pts = fromHalf(half);
  const segs = splineSegs(pts, true);
  const n = half.length;
  const mirrorOf = (i) => 2 * n - 2 - i;
  // hit area: the top edge raised and the lower edge lowered a little
  const lowerL = [half[i1], ...lower, half[i0]];
  const hitL = [...bandSides(along(segs.slice(i0, i1), 4), 1.4).a, ...bandSides(lowerL.map((q) => q.slice(0, 2)), lowerPad).a];
  for (const s of [1, -1]) {
    const [a, b] = s > 0 ? [i0, i1] : [mirrorOf(i1), mirrorOf(i0)];
    const low = s > 0 ? lowerL : [pts[b], ...mirror(lower).reverse(), pts[a]];
    const tail = d.spline(low.map(([x, y], k) => (k === 0 || k === low.length - 1 ? [x, y, 1] : [x, y]))).replace(/^M[^C]*/, '');
    d.control(s > 0 ? left : right, d.hit(d.poly(flip(hitL, s))), [`M${d.p(pts[a])}${d.curve(segs.slice(a, b))}${tail}Z`, ...parts(s)]);
  }
}

/** A plate-shaped control (trigger, back button): a closed spline given for the left side. */
function plateControl(d, name, pts, s, pad = 1) {
  const shape = flip(pts, s);
  d.control(name, d.hit(d.spline(grow(shape, pad), true)), [d.spline(shape, true)]);
}

// ---------------------------------------------------------------- DualSense

/** Left half of the DualSense shell; the Edge has the same shell. */
const DS_SHELL = [
  [0, 0], [-16, 0.4], [-30, 1.6], [-42, 2.7], [-53, 3.8], [-62.2, 6], [-69.4, 12.4], [-73.9, 22.1], [-76.8, 33.8],
  [-78.5, 44], [-79.6, 56], [-80, 70], [-79.6, 84.5], [-78.3, 92.6], [-75.6, 98.6], [-71.6, 102], [-65.6, 104.2],
  [-58.8, 104.6], [-54.8, 101.4], [-53.2, 91.2], [-50.5, 80.5], [-47.8, 75.2], [-45, 71.2], [-41, 68.6], [-34.6, 67.2],
  [-24, 67.3], [-12, 67.8], [0, 68],
];
/** L1 seen from the front: a flat bar standing above the shell's corner, x = -39.5 … -60.9. */
const DS_L1_TOP = [[-39.7, 0.2], [-40.9, -0.8], [-44, -1.3], [-48, -1.1], [-52, -0.3], [-56, 1.15], [-58, 2.1], [-59.8, 3.3], [-60.6, 4.5]];
/** Front silhouette: the shell with L1 on top. */
const DS_HALF = [
  ...DS_SHELL.slice(0, 3),
  [-39.5, topY(DS_SHELL, -39.5), 1],
  ...DS_L1_TOP,
  [-60.9, topY(DS_SHELL, -60.9), 1],
  ...DS_SHELL.slice(5),
];
/** L1's lower edge is the shell edge under it, from the outer end inwards. */
const DS_L1_LOWER = [-58, -55, -51, -47, -43].map((x) => [x, topY(DS_SHELL, x)]);

/** Back silhouette, seen through from the front: L2 stands out above the shell. */
const DS_BACK_HALF = [
  [0, -0.2], [-14, -0.2], [-26, 0.3], [-33, 0.7], [-37.7, 1.9, 1], [-38.3, -1.2], [-39.8, -2.8], [-42, -3.3], [-46, -3.5],
  [-50, -2.9], [-54, -1.8], [-57, -0.6], [-58.6, 0.7], [-59.2, 3], [-59.4, 4.6, 1], [-61.7, 5], [-63.3, 8], [-65.2, 12],
  [-67.7, 18], [-70.7, 26], [-73.5, 35], [-76.3, 45], [-78.6, 55], [-79.8, 65], [-80, 75], [-79.3, 85], [-77.6, 89.4],
  [-75.3, 91.4], [-72.6, 94.5], [-67.4, 98.7], [-62.5, 101.2], [-59.6, 102.3, 1], [-56.5, 98.7], [-53.8, 96],
  [-52.2, 92], [-50.6, 88], [-49, 84], [-47.6, 80], [-46, 76], [-44.3, 72], [-42, 68], [-38.4, 64.3], [-34.3, 63.3],
  [-24, 63.4], [0, 63.4],
];
/** L2 seen from behind, from its outer end back to the inner end (below the shell's edge). */
const DS_L2_LOWER = [[-57.8, 7.4], [-55.5, 10], [-52.3, 11.7], [-47.6, 12.3], [-42.4, 11.7], [-39.5, 10], [-38.1, 7.1]];

/** DualSense-style controller; `edge` adds the Fn buttons below the sticks. */
function dualsense({ edge = false } = {}) {
  const d = new Device(edge ? 'dualsenseEdge' : 'dualsense', {
    title: `DualSense${edge ? ' Edge' : ''}-style controller, front view`,
    file: edge ? 'dualsense-edge-front.svg' : 'dualsense-front.svg',
    scale: 3.3,
    ox: 500,
    oy: 233,
  });
  d.outline(fromHalf(DS_HALF));
  for (const s of [1, -1]) {
    const m = ([x, y]) => [x * s, y];
    // light bar along the touchpad, seam between the dark centre and the grip shell
    d.detail(d.D('M', m([-33.4, 3.2]), 'L', m([-29.4, 28.6]), 'Q', m([-28.4, 35.8]), m([-21.5, 35.8])));
    d.detail(
      d.spline(flip([[-29.6, 32.6], [-35.6, 38.2], [-42, 45.4], [-47.3, 55], [-51.6, 64.6], [-55, 75.2], [-58, 86], [-61.3, 95.2], [-65.8, 102.8]], s)),
    );
  }
  // speaker grille
  for (const [n, y] of [[5, 39.1], [4, 41.7]]) {
    for (let i = 0; i < n; i++) d.detail(d.circle([(i - (n - 1) / 2) * 2.9, y], 0.45));
  }

  // L2 / R2 hide behind L1 / R1; the back view has them.
  seatedShoulder(d, DS_HALF, pointAt(DS_HALF, -39.5), pointAt(DS_HALF, -60.9), DS_L1_LOWER, ['LB', 'RB']);
  const touchpad = d.poly([[-31.2, 1.8, 1.2], [31.2, 1.8, 1.2], [26.6, 33.8, 6], [-26.6, 33.8, 6]]);
  d.control('Touchpad', d.hit(touchpad), [touchpad]);
  for (const s of [1, -1]) {
    const [a, b] = flip([[-38.2, 8.6], [-37.4, 14.2]], s);
    d.control(s > 0 ? 'Back' : 'Start', d.hit(d.capsule(a, b, 3.3)), [d.capsule(a, b, 2)]);
  }

  // d-pad: four separate shield-shaped buttons around an empty hub
  const dc = [-50, 29];
  const arm = [[0, -2.7], [4.9, -6.6, 1.2], [4.9, -13.6, 2.4], [-4.9, -13.6, 2.4], [-4.9, -6.6, 1.2]];
  for (const [name, deg] of [['DpadUp', 0], ['DpadRight', 90], ['DpadDown', 180], ['DpadLeft', 270]]) {
    d.control(name, d.hit(d.wedge(dc, 15.4, deg - 135, deg - 45)), [d.poly(arm.map((q) => move(rot(q, deg), dc)))]);
  }
  const face = faceButtons(d, [49.9, 29], 11.7, 5.2, ['triangle', 'circle', 'cross', 'square'], 4.6, 6.3);
  stick(d, 'LS', 'Left', [-25.9, 51.3], 12.2, 9.4);
  stick(d, 'RS', 'Right', [25.9, 51.3], 12.2, 9.4);
  d.control('Guide', d.hitCircle([0, 50.2], 5.2), [d.circle([0, 50.2], 4.2)]);
  d.control('Misc', d.hit(d.capsule([-4, 59.6], [4, 59.6], 2.6)), [d.capsule([-3.6, 59.6], [3.6, 59.6], 1.1)]);
  if (edge) {
    for (const s of [1, -1]) {
      const [a, b] = flip([[-17, 64.6], [-12, 64.6]], s);
      d.control(s > 0 ? 'PaddleUL' : 'PaddleUR', d.hit(d.capsule(a, b, 2.3)), [d.capsule(a, b, 1.1)]);
    }
  }

  for (const s of [1, -1]) d.label(s > 0 ? 'LB' : 'RB', [-58.6 * s, 2.5], s > 0 ? 'left' : 'right', -12);
  d.label('DpadUp', [-54.9, 19.5], 'left', 16.5);
  d.label('DpadLeft', [-63.6, 29], 'left', 26.5);
  d.label('DpadDown', [-50, 42.6], 'left', 36.5);
  d.label('DpadRight', [-40, 33.9], 'left', 47.5, { via: [[-40.5, 47.5]] });
  d.label('LS', [-38.1, 51.3], 'left', 57);
  d.label('Y', onCircle(face.Y, 5.2, -25), 'right', 16.5);
  d.label('B', [66.8, 29], 'right', 26.5);
  d.label('A', [49.9, 45.9], 'right', 36.5);
  d.label('X', [38.2, 34.2], 'right', 47.5, { via: [[40.5, 47.5]] }); // under the cross
  d.label('RS', [38.1, 51.3], 'right', 57);
  d.label('Back', [-38.2, 6.6], 'top', [-38.2, -32]);
  d.label('Touchpad', [0, 1.8], 'top', [0, -42]);
  d.label('Start', [38.2, 6.6], 'top', [38.2, -32]);
  const [guideAt, miscAt] = edge ? [-23.2, 23.2] : [-24, 24];
  d.label('Guide', onCircle([0, 50.2], 4.2, 135), 'bottom', [guideAt, 113], { via: [[-8.5, 58.5], [-8.5, 72]] });
  d.label('Misc', [4.7, 59.6], 'bottom', [miscAt, 113], { via: [[8.5, 63], [8.5, 72]] });
  if (edge) {
    d.label('PaddleUL', [-14.5, 65.7], 'bottom', [-69.6, 113], { via: [[-14.5, 74]] });
    d.label('PaddleUR', [14.5, 65.7], 'bottom', [69.6, 113], { via: [[14.5, 74]] });
  }
  return d;
}

/**
 * DualSense back: only L2 / R2 show (L1 / R1 hide behind them), plus the Edge's back
 * buttons. The grips end in the dark front shell.
 */
function dualsenseBack({ edge = false } = {}) {
  const d = new Device(edge ? 'dualsenseEdgeBack' : 'dualsenseBack', {
    title: `DualSense${edge ? ' Edge' : ''}-style controller, back view seen through from the front (left shoulder on the left)`,
    file: edge ? 'dualsense-edge-back.svg' : 'dualsense-back.svg',
    scale: 3.3,
    ox: 500,
    oy: 233,
  });
  d.outline(fromHalf(DS_BACK_HALF));
  for (const s of [1, -1]) d.detail(d.spline(flip([[-77.6, 89.4], [-73.6, 90.4], [-69.5, 90.4], [-65.3, 92.4], [-62.2, 95.6], [-60.4, 99.6], [-59.6, 102.3]], s)));
  d.detail(d.circle([-10.7, 48.2], 0.6)); // reset hole
  d.detail(d.capsule([-1, 54.2], [1, 54.2], 0.5));
  seatedShoulder(d, DS_BACK_HALF, pointAt(DS_BACK_HALF, -37.7), pointAt(DS_BACK_HALF, -59.4), DS_L2_LOWER, ['LT', 'RT'], {
    parts: (s) => [d.path(d.spline(flip([[-54.6, 3.6], [-47.6, 3.4], [-40.4, 3.1]], s)), 'stroke-width="1.75" opacity="0.6"')],
  });
  for (const s of [1, -1]) d.label(s > 0 ? 'LT' : 'RT', [-56.4 * s, -1.6], s > 0 ? 'left' : 'right', -10);
  if (edge) {
    const lever = [[-51.5, 52], [-54.6, 61.5], [-56.8, 72]];
    for (const s of [1, -1]) {
      const name = s > 0 ? 'PaddleLL' : 'PaddleLR';
      d.control(name, d.hit(d.band(flip(lever, s), 4.4)), [d.band(flip(lever, s), 3.2)]);
      d.label(name, bandTip(flip(lever, s), 3.2, -1), s > 0 ? 'left' : 'right', 78);
    }
  }
  return d;
}

// ---------------------------------------------------------------- Xbox

const XBOX_HALF = [
  [0, 2.2], [-12, 1.6], [-26, 0.5], [-40, 0.6], [-51, 2.8], [-60, 7.5], [-67, 14.5], [-72, 24], [-75, 36],
  [-76.2, 49], [-75, 62], [-71.5, 76], [-66, 88], [-58.5, 98], [-49, 102.5], [-40, 101], [-33.5, 94], [-30, 84],
  [-26, 76], [-19, 71.5], [-9.5, 69.8], [0, 69.5],
];
const XBOX_LB = [[-19, -3], [-31, -3.4], [-41, -3.2], [-52.8, -0.6], [-62.2, 4.6], [-69.5, 12]];

function xbox() {
  const d = new Device('xbox', {
    title: 'Xbox-style controller, front view',
    file: 'xbox-front.svg',
    scale: 3.4,
    ox: 500,
    oy: 236,
  });
  d.outline(fromHalf(XBOX_HALF));
  d.detail(d.circle([-20, 51], 12.6)); // d-pad dish

  // the triggers sit behind the bumpers; the back view has them
  for (const s of [1, -1]) d.control(s > 0 ? 'LB' : 'RB', d.hit(d.band(flip(XBOX_LB, s), 3.6)), [d.band(flip(XBOX_LB, s), 2.6)]);
  d.control('Guide', d.hitCircle([0, 11.5], 7.8), [d.circle([0, 11.5], 6.6), d.circle([0, 11.5], 4.6, 'stroke-width="1.75" opacity="0.5"')]);
  d.control('Back', d.hitCircle([-12.5, 25], 4.8), [d.circle([-12.5, 25], 3.3)]);
  d.control('Start', d.hitCircle([12.5, 25], 4.8), [d.circle([12.5, 25], 3.3)]);
  d.control('Misc', d.hit(d.capsule([-2.8, 32.5], [2.8, 32.5], 3.4)), [d.capsule([-2.8, 32.5], [2.8, 32.5], 1.8)]);
  crossDpad(d, [-20, 51], 4.1, 10.6, 13.4);
  const face = faceButtons(d, [40, 27], 10, 4.9, ['Y', 'B', 'A', 'X'], 4.4, 6.2);
  stick(d, 'LS', 'Left', [-40, 27], 13.5, 9.6);
  stick(d, 'RS', 'Right', [20, 51], 13.5, 9.6);

  for (const s of [1, -1]) d.label(s > 0 ? 'LB' : 'RB', bandTip(flip(XBOX_LB, s), 2.6, -1), s > 0 ? 'left' : 'right', -12);
  d.label('LS', [-53.5, 27], 'left', 27);
  d.label('DpadUp', [-24.1, 44], 'left', 44);
  d.label('DpadLeft', [-30.6, 51], 'left', 53.5);
  d.label('DpadDown', [-20, 61.6], 'left', 63);
  d.label('DpadRight', [-9.8, 54.7], 'left', 72.5, { via: [[-9, 66]] });
  d.label('Y', [44.9, 17], 'right', 18);
  d.label('B', [54.9, 27], 'right', 27.5);
  d.label('X', [34.9, 27], 'right', 37.5, { via: [[52.5, 35.7]] }); // through the gap between B and A
  d.label('A', onCircle(face.A, 4.9, 45), 'right', 47);
  d.label('RS', [33.5, 51], 'right', 56.5);
  d.label('Guide', [0, 4.9], 'top', [0, -44]);
  d.label('Back', [-12.5, 21.7], 'left', [-16, -24], { via: [[-12.5, -24]] });
  d.label('Start', [12.5, 21.7], 'right', [16, -24], { via: [[12.5, -24]] });
  d.label('Misc', [0, 34.3], 'bottom', [0, 110]);
  return d;
}

/** Four paddles on the grips' backs (Elite-style); shapes for the left side. */
function fourPaddles(d, upper, lower, hw, rows) {
  for (const s of [1, -1]) {
    const [u, l] = s > 0 ? ['PaddleUL', 'PaddleLL'] : ['PaddleUR', 'PaddleLR'];
    d.control(u, d.hit(d.band(flip(upper, s), hw + 1.7)), [d.band(flip(upper, s), hw)]);
    d.control(l, d.hit(d.band(flip(lower, s), hw + 1.7)), [d.band(flip(lower, s), hw)]);
    const side = s > 0 ? 'left' : 'right';
    d.label(u, bandTip(flip(upper, s), hw, -1), side, rows[0]);
    d.label(l, bandTip(flip(lower, s), hw, -1), side, rows[1]);
  }
}

/** Xbox back with the battery cover; `elite` adds the four paddles. */
function xboxBack({ elite = false } = {}) {
  const d = new Device(elite ? 'xboxEliteBack' : 'xboxBack', {
    title: `Xbox${elite ? ' Elite' : ''}-style controller, back view seen through from the front (left shoulder on the left)`,
    file: elite ? 'xbox-elite-back.svg' : 'xbox-back.svg',
    scale: 3.4,
    ox: 500,
    oy: 236,
  });
  d.outline(fromHalf(XBOX_HALF));
  usbPort(d, 5);
  d.detail(d.poly([[-19, 42, 8], [19, 42, 8], [17, 64, 10], [-17, 64, 10]])); // battery cover
  backShoulders(d, {
    bumper: XBOX_LB,
    bumperHw: 2.6,
    trigger: [[-27.5, 5, 3], [-55, 8.4, 4], [-63, 22.5, 6], [-30.5, 19, 5]],
    triggerRow: 22,
  });
  if (elite) {
    fourPaddles(d, [[-25, 38], [-35, 44], [-43.5, 53.5], [-48.5, 65]], [[-31, 62], [-40, 70.5], [-47, 81], [-50.5, 92]], 3.3, [56, 90]);
  }
  return d;
}

// ---------------------------------------------------------------- generic pad

const GENERIC_HALF = [
  [0, 1.5], [-24, 1], [-46, 1.5], [-60, 4.5], [-69, 11], [-73.5, 22], [-75, 36], [-74, 52], [-70.5, 68],
  [-64.5, 83], [-56, 94], [-46.5, 98], [-38, 95.5], [-32.5, 87], [-28.5, 77], [-22, 70], [-11, 67], [0, 66.5],
];
const GENERIC_LB = [[-30, -3.4], [-44, -3.4], [-56, -1.2], [-64.5, 3.6], [-70, 9.5]];

function generic() {
  const d = new Device('generic', {
    title: 'Generic gamepad, front view',
    file: 'generic-front.svg',
    scale: 3.4,
    ox: 500,
    oy: 245,
  });
  d.outline(fromHalf(GENERIC_HALF));
  d.detail(d.circle([-44, 25], 2.2)); // d-pad pivot dimple

  // the triggers sit behind the bumpers; the back view has them
  for (const s of [1, -1]) d.control(s > 0 ? 'LB' : 'RB', d.hit(d.band(flip(GENERIC_LB, s), 3.5)), [d.band(flip(GENERIC_LB, s), 2.5)]);
  d.control('Guide', d.hitCircle([0, 10], 7.2), [d.circle([0, 10], 5.8)]);
  for (const s of [1, -1]) {
    const [a, b] = flip([[-14.6, 22.5], [-9.4, 22.5]], s);
    d.control(s > 0 ? 'Back' : 'Start', d.hit(d.capsule(a, b, 3.6)), [d.capsule(a, b, 1.9)]);
  }
  d.control('Misc', d.hitCircle([0, 31], 3.8), [d.circle([0, 31], 2.4)]);
  crossDpad(d, [-44, 25], 4.3, 11.5, 14);
  const face = faceButtons(d, [44, 25], 10.2, 5, ['Y', 'B', 'A', 'X'], 4.4, 6.2);
  stick(d, 'LS', 'Left', [-21, 48], 12.5, 9);
  stick(d, 'RS', 'Right', [21, 48], 12.5, 9);

  for (const s of [1, -1]) d.label(s > 0 ? 'LB' : 'RB', bandTip(flip(GENERIC_LB, s), 2.5, -1), s > 0 ? 'left' : 'right', -12);
  d.label('DpadUp', [-48.3, 17], 'left', 15);
  d.label('DpadLeft', [-55.5, 25], 'left', 24.5);
  d.label('DpadDown', [-44, 36.5], 'left', 34);
  d.label('DpadRight', [-34, 29.3], 'left', 43.5, { via: [[-36, 43.5]] });
  d.label('LS', [-33.5, 48], 'left', 53);
  d.label('Y', onCircle(face.Y, 5, -20), 'right', 15);
  d.label('B', [59.2, 25], 'right', 24.5);
  d.label('A', onCircle(face.A, 5, 20), 'right', 34);
  d.label('X', onCircle(face.X, 5, 60), 'right', 43.5, { via: [[36, 43.5]] });
  d.label('RS', [33.5, 48], 'right', 53);
  d.label('Guide', [0, 4.2], 'top', [0, -44]);
  d.label('Back', [-12, 20.6], 'left', [-15.5, -24], { via: [[-12, -24]] });
  d.label('Start', [12, 20.6], 'right', [15.5, -24], { via: [[12, -24]] });
  d.label('Misc', [0, 33.4], 'bottom', [0, 106]);
  return d;
}

/** Generic pad back: we cannot know its back buttons, so it offers all four paddles. */
function genericBack() {
  const d = new Device('genericBack', {
    title: 'Generic gamepad, back view seen through from the front (left shoulder on the left)',
    file: 'generic-back.svg',
    scale: 3.4,
    ox: 500,
    oy: 245,
  });
  d.outline(fromHalf(GENERIC_HALF));
  usbPort(d, 4.2);
  backShoulders(d, {
    bumper: GENERIC_LB,
    bumperHw: 2.5,
    trigger: [[-27, 4.4, 3], [-55, 6.8, 4], [-62.5, 20, 6], [-29.5, 17.5, 5]],
    triggerRow: 20,
  });
  fourPaddles(d, [[-25, 36], [-34, 42], [-42, 51], [-47, 62]], [[-31, 63], [-40, 71], [-46.5, 80.5], [-50, 89]], 3.3, [54, 85]);
  return d;
}

// ---------------------------------------------------------------- Nintendo-style pads

/** Small round button, optionally with a glyph (minus, plus, C). */
function roundButton(d, name, c, r, glyph, hitR) {
  const parts = [d.circle(c, r)];
  if (glyph) parts.push(d.path(d.glyph(glyph, c, r * 1.15), 'stroke-width="2"'));
  d.control(name, d.hitCircle(c, hitR), parts);
}

const squareAround = ([x, y], half, r) => [[x - half, y - half, r], [x + half, y - half, r], [x + half, y + half, r], [x - half, y + half, r]];

/** Capture button: a rounded square with a recessed dot. */
function captureButton(d, c, half) {
  d.control('Misc', d.hit(d.poly(squareAround(c, half + 1.5, 1.5))), [
    d.poly(squareAround(c, half, 1)),
    d.circle(c, half * 0.45, 'stroke-width="1.75"'),
  ]);
}

/** Rounded square button with a glyph (the Switch 2 Pro Controller's C button). */
function squareButton(d, name, c, half, glyph, hitHalf) {
  d.control(name, d.hit(d.poly(squareAround(c, hitHalf, 1.6))), [
    d.poly(squareAround(c, half, 1.3)),
    d.path(d.glyph(glyph, c, half * 1.15), 'stroke-width="2"'),
  ]);
}

/** Regular octagon (stick gates). */
const octagon = (c, r) => Array.from({ length: 8 }, (_, i) => onCircle(c, r, 22.5 + i * 45));

// Switch Pro Controller: grips that flare outwards towards their rounded ends. From the
// front, L / R wrap the top corners (x = -24.6 … -62); ZL / ZR hide behind them.
const PRO1_HALF = [
  [0, 0], [-14, 0.1], [-24.6, 0.45, 1], [-26.5, -0.55], [-29, -1.45], [-32, -1.85], [-36, -1.6], [-40, -1], [-44, -0.3],
  [-48, 0.9], [-52, 2.8], [-56, 5.7], [-58, 8.6], [-60, 11.2], [-62, 13.8], [-64, 17.6], [-65.7, 23.4], [-68, 35.5],
  [-70.6, 51], [-73.4, 66.7], [-75.4, 80], [-75.6, 89.5], [-73, 96.4], [-67.8, 101.7], [-62, 103.8], [-55.2, 101.8],
  [-49.2, 95.4], [-46.1, 86.4], [-43.1, 79.3], [-38.8, 74.1], [-33.5, 72.5], [-18, 72.4], [0, 72.4],
];
/** Lower edge of L from the front, from its outer end inwards; the body's top face starts below. */
const PRO1_L_LOWER = [[-60, 12.6], [-57.5, 10], [-55, 8.25], [-52.5, 6.9], [-50, 5.8], [-45, 4.2], [-40, 2.9], [-35, 2.25], [-30, 2], [-25.3, 1.65]];
/** Where the textured grip begins, on the front. */
const PRO1_GRIP_SEAM = [[-67.6, 32.6], [-59.7, 42.7], [-51.6, 55.8], [-44.4, 65.9], [-38.6, 73.4]];

/** Back silhouette, seen through from the front: L / R on top, the ZL / ZR plates below them. */
const PRO1_BACK_HALF = [
  [0, 0.3], [-12, 0.35], [-23.2, 0.45, 1], [-25, 0.05], [-27, -0.4], [-29, -0.85], [-32, -0.95], [-36, -0.55], [-40, 0],
  [-44, 0.8], [-48, 2.15], [-52, 4.3], [-56, 7.85], [-60, 11.6], [-62.4, 15], [-64.5, 20], [-66.3, 26], [-68.3, 35],
  [-70.2, 45], [-71.9, 55], [-73.5, 65], [-74.7, 75], [-75.5, 85], [-75.3, 91], [-74.5, 95], [-72, 100], [-69, 102.9],
  [-66, 104.4], [-62.5, 105.3], [-59, 105], [-56, 103.8], [-53, 101.2], [-51, 98.5], [-49.6, 96], [-47.8, 92], [-46, 88],
  [-44.5, 84], [-43, 80], [-41.5, 76], [-39.8, 72], [-37.6, 68], [-35.5, 65.6], [-32.5, 64.6], [-24, 64.5], [0, 64.5],
];
const PRO1_BACK_L_LOWER = [[-59.6, 14.3], [-57.6, 11.6], [-55.3, 8.9], [-52.9, 7], [-49.7, 5.7], [-45.5, 5], [-40.2, 4.8], [-35.4, 4.8], [-31.2, 5.2], [-27.3, 5.7]];
const PRO1_BACK_ZL = [
  [-30.1, 7.8], [-34.4, 7.6], [-40.2, 7.8], [-45.5, 8.4], [-50.8, 9.1], [-55.5, 11.3], [-57.9, 13.9], [-57.1, 17.1], [-54.7, 19.2],
  [-50.8, 20.3], [-45.5, 20.5], [-41.8, 19.9], [-38.6, 18.6], [-35.2, 18.4], [-32.8, 12.9],
];
/** Where the textured grip begins, on the back. */
const PRO1_BACK_GRIP_SEAM = [[-67.2, 33.1], [-60.9, 35], [-52.2, 38.5], [-45.9, 42.2], [-41.6, 46.9], [-38.4, 52.5], [-36.6, 58.1], [-35.9, 64.8]];

// Switch 2 Pro Controller: rounder top corners, longer grips, a raised front panel. Its
// white L / R show on the top corners (x = -23.6 … -59.6).
const PRO2_HALF = [
  [0, 0], [-14, 0.2], [-23.6, 0.35, 1], [-26, -0.8], [-28.5, -1.7], [-31, -1.95], [-34, -1.75], [-38, -1.3], [-42, -0.7],
  [-46, 0.3], [-50, 1.8], [-54, 4.3], [-56, 6], [-58, 8.6], [-59.6, 11.3], [-62, 14.6], [-64.4, 21.2], [-66.6, 29],
  [-68.8, 37.2], [-71.6, 54.2], [-74.4, 70.8], [-75.5, 80], [-75.6, 87.5], [-73.8, 95.4], [-69.4, 102.2], [-61.2, 104.7],
  [-53.8, 102.2], [-47.8, 95.4], [-44.1, 87.5], [-40.4, 79.2], [-37.5, 73.9], [-33.4, 72.8], [-18, 72.8], [0, 72.8],
];
const PRO2_L_LOWER = [[-58, 10.5], [-56, 9.3], [-54, 8.1], [-52, 7], [-50, 6.1], [-48, 5.3], [-46, 4.7], [-44, 4.2], [-42, 3.7], [-40, 3.4], [-36, 2.7], [-32, 2.3], [-28, 2], [-24.2, 1.8]];

const PRO2_BACK_HALF = [
  [0, 0.15], [-12, 0.15], [-21.8, 0.3, 1], [-24, -0.6], [-27, -1.5], [-30, -2], [-34, -1.5], [-38, -0.95], [-42, -0.4],
  [-46, 0.7], [-50, 2.7], [-54, 5.45], [-57, 9.1], [-60, 12.45], [-61.9, 15], [-64.1, 20], [-65.5, 25], [-66.7, 30],
  [-68.6, 40], [-70.6, 50], [-72, 60], [-73.6, 70], [-75, 80], [-75.6, 90], [-75.2, 97], [-74, 101], [-72, 104.4],
  [-69, 107.7], [-66, 109.2], [-62.5, 110], [-59, 109.6], [-56.5, 108.4], [-54, 106.6], [-51, 102.4], [-48, 97.1],
  [-45, 90.1], [-42.6, 84], [-40.9, 80], [-39.3, 76], [-36.8, 72], [-34.5, 70.3], [-31, 69.7], [0, 69.7],
];
const PRO2_BACK_L_LOWER = [[-58.9, 12.9], [-57.9, 11.8], [-55.7, 9.4], [-52.2, 7], [-47.8, 5.5], [-42.8, 4.8], [-37.7, 4.6], [-32.7, 4.3], [-28.9, 3.7], [-24.5, 2.6]];
const PRO2_BACK_ZL = [
  [-31.6, 5.6], [-29.9, 7.3], [-30.5, 10.8], [-32.1, 14.6], [-33.5, 17.2], [-36.5, 17.8], [-42.8, 17.9], [-49.1, 17.4],
  [-54.8, 16], [-57.6, 14.3], [-57.4, 12.6], [-55.4, 10.4], [-52.1, 8.2], [-47.8, 6.8], [-42.8, 6], [-37, 5.8],
];
/** GL / GR on the grips' inner backs. */
const PRO2_BACK_GL = [
  [-45.5, 50.8], [-42.5, 51.9], [-39.1, 56.6], [-36.3, 65.2], [-37, 71.1], [-40.8, 73.6], [-44.3, 71.3], [-50, 65.2],
  [-51.8, 60.4], [-50.3, 54.5], [-47.7, 51.4],
];

/** Switch Pro Controller (first generation): no C button, no back buttons. */
function switchPro() {
  const d = new Device('switchpro', {
    title: 'Switch Pro-style controller, front view',
    file: 'switchpro-front.svg',
    scale: 3.4,
    ox: 500,
    oy: 228,
  });
  d.outline(fromHalf(PRO1_HALF));
  d.detail(d.circle([10.9, 29.8], 4.6)); // ring around Home
  for (const s of [1, -1]) d.detail(d.spline(flip(PRO1_GRIP_SEAM, s)));
  d.detail(d.spline([[-25.3, 1.65], [0, 1.45], [25.3, 1.65]])); // the top face between L and R

  seatedShoulder(d, PRO1_HALF, pointAt(PRO1_HALF, -24.6), pointAt(PRO1_HALF, -62), PRO1_L_LOWER, ['LB', 'RB']);
  roundButton(d, 'Back', [-18.9, 18.1], 3, 'minus', 4.4);
  roundButton(d, 'Start', [18.9, 18.1], 3, 'plus', 4.4);
  captureButton(d, [-10.9, 29.8], 2.9);
  roundButton(d, 'Guide', [10.9, 29.8], 3.3, 'home', 4.6);
  crossDpad(d, [-23.2, 50.9], 4.2, 11.7, 13.4, { arrows: true, recess: 1.4 });
  const face = faceButtons(d, [40.6, 29.8], [12, 10.4], 5.6, ['X', 'A', 'B', 'Y'], 4.6, 6.6);
  stick(d, 'LS', 'Left', [-41.9, 29.7], 12.4, 8.8);
  stick(d, 'RS', 'Right', [20.4, 50.2], 12.4, 8.8);

  for (const s of [1, -1]) d.label(s > 0 ? 'LB' : 'RB', [-56.3 * s, 5.8], s > 0 ? 'left' : 'right', -12);
  d.label('LS', [-54.3, 29.7], 'left', 27);
  d.label('DpadUp', [-27.4, 44.5], 'left', 44.5);
  d.label('DpadLeft', [-34.9, 50.9], 'left', 54);
  d.label('DpadDown', [-23.2, 62.6], 'left', 63.5);
  d.label('DpadRight', [-13, 55.1], 'left', 73, { via: [[-12.6, 66.5]] });
  d.label('Y', onCircle(face.Y, 5.6, -25), 'right', 17);
  d.label('B', [58.2, 29.8], 'right', 27.5);
  d.label('A', onCircle(face.A, 5.6, 30), 'right', 38);
  d.label('X', onCircle(face.X, 5.6, 60), 'right', 48.4, { via: [[31.6, 40.5], [35, 48.4]] }); // between A and the right stick
  d.label('RS', [32.8, 50.2], 'right', 58);
  d.label('Back', [-18.9, 15.1], 'left', [-21.5, -24], { via: [[-18.9, -24]] });
  d.label('Start', [18.9, 15.1], 'right', [21.5, -24], { via: [[18.9, -24]] });
  d.label('Guide', onCircle([10.9, 29.8], 3.3, 200), 'top', [0, -44], { via: [[0, 22]] });
  d.label('Misc', [-10.9, 32.7], 'bottom', [0, 112], { via: [[-6.5, 37], [-6.5, 80], [0, 86]] });
  return d;
}

/** Switch 2 Pro Controller: the C button below the middle, GL / GR on the back. */
function switch2Pro() {
  const d = new Device('switch2pro', {
    title: 'Switch 2 Pro-style controller, front view',
    file: 'switch2pro-front.svg',
    scale: 3.4,
    ox: 500,
    oy: 226,
  });
  d.outline(fromHalf(PRO2_HALF));
  // edge of the raised front panel
  const panel = [[-59.2, 11.4], [-61.4, 24], [-61.9, 36], [-60, 48.4], [-54.8, 58.8], [-46.5, 66], [-36.1, 68.7], [-18, 69.2], [0, 69.3]];
  d.detail(d.spline([...panel, ...mirror(panel.slice(0, -1)).reverse()]));
  d.detail(d.spline([[-24.2, 1.8], [0, 1.5], [24.2, 1.8]])); // the top face between L and R

  seatedShoulder(d, PRO2_HALF, pointAt(PRO2_HALF, -23.6), pointAt(PRO2_HALF, -59.6), PRO2_L_LOWER, ['LB', 'RB']);
  roundButton(d, 'Back', [-18.25, 17.6], 3.3, 'minus', 4.7);
  roundButton(d, 'Start', [18.25, 17.6], 3.3, 'plus', 4.7);
  captureButton(d, [-9.2, 29.5], 3.1);
  roundButton(d, 'Guide', [9.2, 29.5], 3.7, 'home', 5);
  squareButton(d, 'Misc2', [0, 62.6], 3, 'C', 4.4);
  crossDpad(d, [-22.1, 51], 4.3, 11, 13.4, { arrows: true, recess: 1.5 });
  const face = faceButtons(d, [39.1, 29.6], [12.25, 10.5], 6.1, ['X', 'A', 'B', 'Y'], 4.8, 7.2);
  stick(d, 'LS', 'Left', [-41.1, 29.7], 12.7, 9.4);
  stick(d, 'RS', 'Right', [18.4, 51], 12.7, 9.4);

  for (const s of [1, -1]) d.label(s > 0 ? 'LB' : 'RB', [-55.4 * s, 5.3], s > 0 ? 'left' : 'right', -12);
  d.label('LS', [-53.8, 29.7], 'left', 27);
  d.label('DpadUp', [-26.4, 44.2], 'left', 44.2);
  d.label('DpadLeft', [-33.1, 51], 'left', 53.4);
  d.label('DpadDown', [-22.1, 62], 'left', 62.6);
  d.label('DpadRight', [-13, 55.3], 'left', 71.8, { via: [[-12.6, 67]] });
  d.label('Y', onCircle(face.Y, 6.1, -25), 'right', 16.5);
  d.label('B', [57.45, 29.6], 'right', 27);
  d.label('A', onCircle(face.A, 6.1, 30), 'right', 37.5);
  d.label('X', onCircle(face.X, 6.1, 60), 'right', 48.6, { via: [[29.6, 40.5], [33.6, 48.6]] }); // between A and the right stick
  d.label('RS', [31.1, 51], 'right', 58);
  d.label('Back', [-18.25, 14.3], 'left', [-21, -24], { via: [[-18.25, -24]] });
  d.label('Start', [18.25, 14.3], 'right', [21, -24], { via: [[18.25, -24]] });
  d.label('Guide', onCircle([9.2, 29.5], 3.7, 200), 'top', [0, -44], { via: [[0, 22]] });
  d.label('Misc', [-9.2, 32.6], 'bottom', [-24, 112], { via: [[-6.5, 37], [-6.5, 78]] });
  d.label('Misc2', [0, 65.6], 'bottom', [24, 112], { via: [[0, 78]] });
  return d;
}

/** Back of either Pro Controller; the Switch 2 one has GL / GR. */
function switchProBack({ gen2 = false } = {}) {
  const d = new Device(gen2 ? 'switch2proBack' : 'switchproBack', {
    title: `Switch ${gen2 ? '2 ' : ''}Pro-style controller, back view seen through from the front (left shoulder on the left)`,
    file: gen2 ? 'switch2pro-back.svg' : 'switchpro-back.svg',
    scale: 3.4,
    ox: 500,
    oy: gen2 ? 226 : 228,
  });
  const half = gen2 ? PRO2_BACK_HALF : PRO1_BACK_HALF;
  d.outline(fromHalf(half));
  if (gen2) {
    d.detail(d.capsule([-3.6, 4.5], [3.6, 4.5], 0.7)); // USB port
    d.detail(d.circle([-11.2, 4.7], 1.4)); // pairing button
    d.detail(d.circle([11.2, 4.5], 0.35));
    for (const x of [-6.2, -2, 2.1, 6.1]) d.detail(d.capsule([x - 0.6, 1.4], [x + 0.6, 1.4], 0.25)); // player lights
    d.detail(d.spline([[-33.4, 16.8], [0, 16.4], [33.4, 16.8]])); // lower edge of the light top
  } else {
    d.detail(d.capsule([-3.9, 4.35], [3.9, 4.35], 0.85)); // USB port
    d.detail(d.circle([-10.1, 4.4], 1.1)); // pairing button
    d.detail(d.circle([10, 4.3], 0.3));
    d.detail(d.D('M', [-35, 20.2], 'L', [35, 20.2])); // lower edge of the dark top panel
    for (const s of [1, -1]) d.detail(d.spline(flip(PRO1_BACK_GRIP_SEAM, s)));
  }
  // L / R from behind: a band along the top corner; ZL / ZR: a plate below it
  const [i0, i1] = gen2 ? [pointAt(half, -21.8), pointAt(half, -60)] : [pointAt(half, -23.2), pointAt(half, -62.4)];
  seatedShoulder(d, half, i0, i1, gen2 ? PRO2_BACK_L_LOWER : PRO1_BACK_L_LOWER, ['LB', 'RB'], { lowerPad: 0 });
  for (const s of [1, -1]) plateControl(d, s > 0 ? 'LT' : 'RT', gen2 ? PRO2_BACK_ZL : PRO1_BACK_ZL, s, 0.4);
  if (gen2) for (const s of [1, -1]) plateControl(d, s > 0 ? 'PaddleUL' : 'PaddleUR', PRO2_BACK_GL, s);

  for (const s of [1, -1]) {
    const side = s > 0 ? 'left' : 'right';
    d.label(s > 0 ? 'LB' : 'RB', gen2 ? [-55 * s, 6.4] : [-55.8 * s, 7.5], side, -8);
    d.label(s > 0 ? 'LT' : 'RT', gen2 ? [-55.6 * s, 15.7] : [-56.2 * s, 18.4], side, 22);
    if (gen2) d.label(s > 0 ? 'PaddleUL' : 'PaddleUR', [-52.6 * s, 60.4], side, 60);
  }
  return d;
}

/** Left Joy-Con outline (the right one is its mirror); the straight inner edge is the rail. */
const JOYCON_HALF = [
  [-6, 1, 1], [-22, 1], [-33, 4], [-39.5, 10.5], [-42.5, 22], [-43.2, 51], [-42.5, 80], [-39.5, 91.5], [-33, 98],
  [-22, 101], [-6, 101, 1],
];
const JOYCON_L = [[-16.5, -2.6], [-24, -2.6], [-33, -0.6], [-39.5, 4], [-44, 10.5]];

/** SL / SR: slim buttons on a Joy-Con's rail. */
function railButton(d, name, a, b) {
  d.control(name, d.hit(d.capsule(a, b, 2.6)), [d.capsule(a, b, 1.4)]);
}

/** A left and a right Joy-Con side by side (a combined pair); Joy-Con 2 have a C button. */
function joyCons({ gen2 = false } = {}) {
  const d = new Device(gen2 ? 'joycon2' : 'joycon', {
    title: `Joy-Con${gen2 ? ' 2' : ''} pair, front view`,
    file: gen2 ? 'joycon2-front.svg' : 'joycon-front.svg',
    scale: 3.4,
    ox: 500,
    oy: 210,
  });
  for (const s of [1, -1]) {
    d.outline(flip(JOYCON_HALF, s));
    d.detail(d.D('M', flip([[-8.6, 8]], s)[0], 'L', flip([[-8.6, 94]], s)[0])); // rail
    for (const y of [44, 49, 54, 59]) d.detail(d.circle([-10.2 * s, y], 0.5)); // player lights
    // ZL / ZR sit behind L / R; the back view has them
    d.control(s > 0 ? 'LB' : 'RB', d.hit(d.band(flip(JOYCON_L, s), 3.2)), [d.band(flip(JOYCON_L, s), 2.2)]);
  }

  // Left Joy-Con
  roundButton(d, 'Back', [-12, 9], 2.6, 'minus', 3.6);
  stick(d, 'LS', 'Left', [-24, 28], 11, 8);
  const pad = [-24, 58];
  for (const [name, dir, dx, dy] of [
    ['DpadUp', 'up', 0, -8],
    ['DpadRight', 'right', 8, 0],
    ['DpadDown', 'down', 0, 8],
    ['DpadLeft', 'left', -8, 0],
  ]) {
    const c = [pad[0] + dx, pad[1] + dy];
    d.control(name, d.hitCircle(c, 4.6), [d.circle(c, 3.6), d.path(d.glyph(`arrow-${dir}`, c, 3), 'stroke-width="2"')]);
  }
  captureButton(d, [-15, 80], 2.5);
  railButton(d, 'PaddleUL', [-7.6, 22], [-7.6, 33]); // SL
  railButton(d, 'PaddleLL', [-7.6, 69], [-7.6, 80]); // SR

  // Right Joy-Con
  roundButton(d, 'Start', [12, 9], 2.6, 'plus', 3.6);
  const face = faceButtons(d, [24, 28], 8, 3.6, ['X', 'A', 'B', 'Y'], 3.2, 4.6);
  stick(d, 'RS', 'Right', [24, 58], 11, 8);
  d.detail(d.circle([15, 77], 4.2)); // ring around Home
  roundButton(d, 'Guide', [15, 77], 3, 'home', 4.2);
  if (gen2) roundButton(d, 'Misc2', [15, 87], 2.8, 'C', 3.9);
  railButton(d, 'PaddleUR', [7.6, 22], [7.6, 33]); // SR
  railButton(d, 'PaddleLR', [7.6, 69], [7.6, 80]); // SL

  for (const s of [1, -1]) d.label(s > 0 ? 'LB' : 'RB', bandTip(flip(JOYCON_L, s), 2.2, -1), s > 0 ? 'left' : 'right', 3);
  d.label('LS', [-35, 28], 'left', 28);
  d.label('DpadUp', onCircle([-24, 50], 3.6, 200), 'left', 46);
  d.label('DpadLeft', [-35.6, 58], 'left', 56);
  d.label('DpadDown', onCircle([-24, 66], 3.6, 160), 'left', 66);
  d.label('DpadRight', [-16, 61.6], 'left', 75.5, { via: [[-16, 72.5]] });
  d.label('Y', onCircle(face.Y, 3.6, -20), 'right', 17);
  d.label('B', [35.6, 28], 'right', 27);
  d.label('A', onCircle(face.A, 3.6, 20), 'right', 37);
  d.label('X', onCircle(face.X, 3.6, 110), 'right', 47, { via: [[20, 43]] }); // under A
  d.label('RS', [35, 58], 'right', 57);
  d.label('Back', [-12, 6.4], 'top', [-70.6, -40], { via: [[-12, -17]] });
  d.label('PaddleUL', [-7.6, 20.6], 'top', [-23.5, -40], { via: [[-7.6, -20]] });
  d.label('PaddleUR', [7.6, 20.6], 'top', [23.5, -40], { via: [[7.6, -20]] });
  d.label('Start', [12, 6.4], 'top', [70.6, -40], { via: [[12, -17]] });
  d.label('Misc', [-15, 82.5], 'bottom', [-94.1, 116], { via: [[-15, 102]] });
  d.label('PaddleLL', [-7.6, 81.4], 'bottom', [-47.1, 116], { via: [[-7.6, 110]] });
  d.label('PaddleLR', [7.6, 81.4], 'bottom', [0, 116], { via: [[7.6, 110]] });
  if (gen2) d.label('Misc2', [15, 89.8], 'bottom', [47.1, 116], { via: [[15, 104]] });
  d.label('Guide', onCircle([15, 77], 3, 20), 'bottom', [94.1, 116], { via: [[34, 96]] });
  return d;
}

/** Back of a Joy-Con pair: L / R on the edges, ZL / ZR behind them, SL / SR on the rails. */
function joyConsBack() {
  const d = new Device('joyconBack', {
    title: 'Joy-Con pair, back view seen through from the front (left Joy-Con on the left)',
    file: 'joycon-back.svg',
    scale: 3.4,
    ox: 500,
    oy: 210,
  });
  for (const s of [1, -1]) {
    d.outline(flip(JOYCON_HALF, s));
    d.detail(d.D('M', flip([[-8.6, 8]], s)[0], 'L', flip([[-8.6, 94]], s)[0])); // rail
  }
  backShoulders(d, {
    bumper: JOYCON_L,
    bumperHw: 2.2,
    trigger: [[-13.5, 4.4, 2.5], [-33, 7.4, 3.5], [-38.5, 19, 5], [-14.5, 16.5, 4]],
    triggerRow: 22,
  });
  railButton(d, 'PaddleUL', [-7.6, 22], [-7.6, 33]); // left SL
  railButton(d, 'PaddleLL', [-7.6, 69], [-7.6, 80]); // left SR
  railButton(d, 'PaddleUR', [7.6, 22], [7.6, 33]); // right SR
  railButton(d, 'PaddleLR', [7.6, 69], [7.6, 80]); // right SL
  for (const s of [1, -1]) {
    const [u, l] = s > 0 ? ['PaddleUL', 'PaddleLL'] : ['PaddleUR', 'PaddleLR'];
    d.label(u, [-9 * s, 27.5], s > 0 ? 'left' : 'right', 32);
    d.label(l, [-9 * s, 74.5], s > 0 ? 'left' : 'right', 74.5);
  }
  return d;
}

const GC_HALF = [
  [0, 4], [-14, 3.2], [-28, 2.2], [-42, 3.5], [-53, 8], [-61, 15], [-66, 25], [-68, 37], [-68.5, 50], [-67, 63],
  [-63, 75], [-57, 84], [-49, 88], [-41, 86], [-35, 78], [-31, 68], [-26, 61], [-16, 58], [0, 57],
];
const GC_TRIGGER = [[-30, -11.5], [-43, -11.8], [-53, -9.6], [-60, -5.6]];
const GC_BUMPER = [[-24, -2.8], [-37, -3.2], [-48, -1.6], [-56.5, 2.8]];

/**
 * GameCube-style controller. Its face buttons are mapped by position: the big A is south,
 * B (left of it) west, X (right) east and Y (top) north.
 */
function gameCube() {
  const d = new Device('gamecube', {
    title: 'GameCube-style controller, front view',
    file: 'gamecube-front.svg',
    scale: 3.4,
    ox: 500,
    oy: 220,
  });
  d.outline(fromHalf(GC_HALF));
  d.detail(d.poly(octagon([-36, 28], 13.2))); // stick gate
  for (const s of [1, -1]) {
    d.control(s > 0 ? 'LT' : 'RT', d.hit(d.band(flip(GC_TRIGGER, s), 4.8)), [d.band(flip(GC_TRIGGER, s), 4)]);
    d.control(s > 0 ? 'LB' : 'RB', d.hit(d.band(flip(GC_BUMPER, s), 3.3)), [d.band(flip(GC_BUMPER, s), 2.3)]);
  }
  stick(d, 'LS', 'Left', [-36, 28], 11.5, 8.4);
  stick(d, 'RS', 'Right', [21, 50], 8.2, 5.8);
  crossDpad(d, [-22, 48], 3.2, 8, 10);
  captureButton(d, [-9, 14], 2.3);
  roundButton(d, 'Guide', [9, 14], 2.8, 'home', 4);
  roundButton(d, 'Start', [0, 25], 3, null, 4.2);
  roundButton(d, 'Misc2', [0, 35], 2.8, 'C', 3.9);

  const A = [41, 28];
  d.control('A', d.hitCircle(A, 8), [d.circle(A, 7), d.path(d.glyph('A', A, 5.4), 'stroke-width="2.5"')]);
  const B = [29.5, 37.5];
  d.control('X', d.hitCircle(B, 5), [d.circle(B, 3.9), d.path(d.glyph('B', B, 3.6), 'stroke-width="2.5"')]);
  const arc = (a0, a1) => [0, 1, 2, 3].map((i) => onCircle(A, 13.5, a0 + ((a1 - a0) * i) / 3));
  const kidney = (name, letter, a0, a1) => {
    const pts = arc(a0, a1);
    const mid = onCircle(A, 13.5, (a0 + a1) / 2);
    d.control(name, d.hit(d.band(pts, 4.2)), [d.band(pts, 3.2), d.path(d.glyph(letter, mid, 3.2), 'stroke-width="2.5"')]);
  };
  kidney('B', 'X', -55, 4);
  kidney('Y', 'Y', -160, -100);

  for (const s of [1, -1]) {
    const side = s > 0 ? 'left' : 'right';
    d.label(s > 0 ? 'LT' : 'RT', bandEdge(flip(GC_TRIGGER, s), 4, 2, s), side, -27);
    d.label(s > 0 ? 'LB' : 'RB', bandTip(flip(GC_BUMPER, s), 2.3, -1), side, -10);
  }
  d.label('LS', [-47.5, 28], 'left', 25);
  d.label('DpadUp', [-25.2, 42], 'left', 41.5);
  d.label('DpadLeft', [-30, 48], 'left', 51);
  d.label('DpadDown', [-22, 56], 'left', 60.5);
  d.label('DpadRight', [-14, 51], 'left', 70, { via: [[-13.5, 63]] });
  d.label('Y', onCircle(A, 16.7, -104), 'right', 6);
  d.label('B', onCircle(A, 16.7, -25), 'right', 22);
  d.label('A', onCircle(A, 7, 60), 'right', 37);
  d.label('X', [29.5, 41.4], 'right', 46, { via: [[37, 45.5]] });
  d.label('RS', [29.2, 50], 'right', 56);
  d.label('Misc', [-9, 11.7], 'top', [-24, -40], { via: [[-9, -18]] });
  d.label('Guide', [9, 11.2], 'top', [24, -40], { via: [[9, -18]] });
  d.label('Start', onCircle([0, 25], 3, 135), 'bottom', [-24, 98], { via: [[-6, 33], [-6, 70]] });
  d.label('Misc2', onCircle([0, 35], 2.8, 60), 'bottom', [24, 98], { via: [[5, 42], [5, 70]] });
  return d;
}

/** GameCube back: ZL / Z on the edge, the analog L / R behind them. */
function gameCubeBack() {
  const d = new Device('gamecubeBack', {
    title: 'GameCube-style controller, back view seen through from the front (left shoulder on the left)',
    file: 'gamecube-back.svg',
    scale: 3.4,
    ox: 500,
    oy: 220,
  });
  d.outline(fromHalf(GC_HALF));
  usbPort(d, 6.4);
  backShoulders(d, {
    bumper: GC_BUMPER,
    bumperHw: 2.3,
    trigger: [[-25.5, 6, 3], [-50, 9.4, 4], [-57, 21, 6], [-28, 18, 5]],
  });
  return d;
}

// ---------------------------------------------------------------- mouse

function mouse() {
  const d = new Device('mouse', {
    title: 'Mouse, top view',
    file: 'mouse.svg',
    scale: 4,
    ox: 500,
    oy: 114,
    colL: 250,
    colR: 750,
  });
  d.outline(
    fromHalf([
      [0, 0], [-11, 0.6], [-19.5, 2.8], [-25, 7.5], [-28, 15], [-29.5, 26], [-29.8, 38], [-29.4, 50], [-29.8, 62],
      [-31, 75], [-31.8, 88], [-31, 100], [-27.5, 110], [-20, 117.5], [-10, 121.3], [0, 122],
    ]),
  );
  d.line(d.poly([[-5, 13.2, 4.2], [5, 13.2, 4.2], [5, 36.8, 4.2], [-5, 36.8, 4.2]])); // wheel slot
  d.line(d.capsule([0, 17.6], [0, 32.4], 3)); // wheel

  const panel = [
    [-2, 1.9, 1], [-10.5, 2.5], [-18.4, 4.6], [-23.4, 8.8], [-26.2, 15.5], [-27.6, 26], [-27.9, 38], [-27.5, 47.4, 1],
    [-15, 51], [-2, 52.4, 1], [-2, 39.4, 1], [-7, 37.9, 1], [-7, 12.1, 1], [-2, 10.6, 1],
  ];
  for (const s of [1, -1]) {
    const shape = d.spline(flip(panel, s), true);
    d.control(s > 0 ? 'Mouse:Left' : 'Mouse:Right', d.hit(shape), [shape]);
  }
  d.control('Wheel:Up', d.hit(d.poly([[-4.6, 13.7, 3.6], [4.6, 13.7, 3.6], [4.6, 21.8], [-4.6, 21.8]])), [
    d.D('M', [-1.6, 20], 'L', [0, 18.2], 'L', [1.6, 20]),
  ]);
  d.control('Mouse:Middle', d.hit(d.poly([[-4.6, 21.8], [4.6, 21.8], [4.6, 28.2], [-4.6, 28.2]])), [
    d.path(d.D('M', [-1.7, 23.6], 'L', [1.7, 23.6], 'M', [-1.7, 25], 'L', [1.7, 25], 'M', [-1.7, 26.4], 'L', [1.7, 26.4]), 'stroke-width="2"'),
  ]);
  d.control('Wheel:Down', d.hit(d.poly([[-4.6, 28.2], [4.6, 28.2], [4.6, 36.3, 3.6], [-4.6, 36.3, 3.6]])), [
    d.D('M', [-1.6, 30], 'L', [0, 31.8], 'L', [1.6, 30]),
  ]);
  d.control('Mouse:X2', d.hit(d.capsule([-33.4, 56.5], [-33.6, 67], 3.2)), [d.capsule([-33.4, 56.5], [-33.6, 67], 2.1)]);
  d.control('Mouse:X1', d.hit(d.capsule([-34.8, 74], [-35.2, 84.5], 3.2)), [d.capsule([-34.8, 74], [-35.2, 84.5], 2.1)]);

  d.label('Mouse:Left', [-15, 27], 'left', 24);
  d.label('Mouse:X2', [-35.6, 61.75], 'left', 60);
  d.label('Mouse:X1', [-37.1, 79.25], 'left', 80);
  d.label('Mouse:Right', [16, 14], 'right', 12);
  d.label('Mouse:Middle', [1.7, 25], 'right', 34, { allow: ['Mouse:Right'] });
  d.label('Wheel:Up', [0, 18.2], 'top', [0, -13]);
  d.label('Wheel:Down', [0, 31.8], 'bottom', [0, 131]);
  return d;
}

// ---------------------------------------------------------------- checks

/** Minimal XML well-formedness check: balanced tags, quoted unique attributes, escaped text. */
function checkXml(text) {
  const re =
    /<!--([\s\S]*?)-->|<\?[\s\S]*?\?>|<(\/?)([A-Za-z][\w:.-]*)((?:\s+[\w:.-]+\s*=\s*(?:"[^"<]*"|'[^'<]*'))*)\s*(\/?)>|<|&(?!(?:amp|lt|gt|quot|apos|#\d+|#x[\da-fA-F]+);)/g;
  const stack = [];
  let roots = 0;
  let last = 0;
  for (let m; (m = re.exec(text)); ) {
    const between = text.slice(last, m.index);
    if (!stack.length && between.trim()) throw new Error(`text outside the root element at ${last}`);
    last = re.lastIndex;
    if (m[0].startsWith('<!--')) {
      if (m[1].includes('--')) throw new Error(`"--" inside a comment at ${m.index}`);
      continue;
    }
    if (m[0].startsWith('<?')) continue;
    if (m[0] === '<') throw new Error(`stray "<" at ${m.index}`);
    if (m[0].startsWith('&')) throw new Error(`unescaped "&" at ${m.index}`);
    const [, , close, name, attrs, self] = m;
    const names = [...attrs.matchAll(/([\w:.-]+)\s*=/g)].map((a) => a[1]);
    if (new Set(names).size !== names.length) throw new Error(`duplicate attribute in <${name}> at ${m.index}`);
    if (close) {
      if (attrs.trim() || self) throw new Error(`malformed </${name}> at ${m.index}`);
      const open = stack.pop();
      if (open !== name) throw new Error(`</${name}> closes <${open}> at ${m.index}`);
    } else {
      if (!stack.length && ++roots > 1) throw new Error(`second root element at ${m.index}`);
      if (!self) stack.push(name);
    }
  }
  if (stack.length) throw new Error(`unclosed <${stack.at(-1)}>`);
  if (text.slice(last).trim()) throw new Error('text after the root element');
  if (!roots) throw new Error('no root element');
}

function chipRect([x, y], side) {
  const { w, h } = CHIP;
  return {
    left: { x: x - w, y: y - h / 2 },
    right: { x, y: y - h / 2 },
    top: { x: x - w / 2, y: y - h },
    bottom: { x: x - w / 2, y },
  }[side];
}

function segmentsCross(a, b, c, e) {
  const o = (p, q, r) => (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
  return o(a, b, c) * o(a, b, e) < 0 && o(c, e, a) * o(c, e, b) < 0;
}

function checkDevice(d, svgText) {
  const problems = [];
  try {
    checkXml(svgText);
  } catch (e) {
    problems.push(`${d.file}: not well-formed XML: ${e.message}`);
  }
  const inSvg = [...svgText.matchAll(/data-control="([^"]+)"/g)].map((m) => m[1]);
  const inLayout = d.labels.map((l) => l.control);
  const dupes = (list) => list.filter((x, i) => list.indexOf(x) !== i);
  for (const x of dupes(inSvg)) problems.push(`${d.file}: data-control "${x}" appears twice`);
  for (const x of dupes(inLayout)) problems.push(`${d.id} layout: "${x}" listed twice`);
  for (const x of inLayout) if (!inSvg.includes(x)) problems.push(`${d.id} layout: "${x}" has no data-control in ${d.file}`);
  for (const x of inSvg) if (!inLayout.includes(x)) problems.push(`${d.file}: "${x}" has no label in the layout`);
  for (const x of EXPECTED[d.id]) if (!inSvg.includes(x)) problems.push(`${d.file}: expected control "${x}" is missing`);
  for (const x of inSvg) if (!EXPECTED[d.id].includes(x)) problems.push(`${d.file}: unexpected control "${x}"`);
  for (const c of d.controls) if (!c.hit.includes('class="hit"')) problems.push(`${d.file}: "${c.name}" has no hit area`);
  for (const [name, side] of [['LS', 'Left'], ['RS', 'Right']]) {
    if (inSvg.includes(name) && !svgText.includes(`data-control="${name}" data-stick="${side}"`)) problems.push(`${d.file}: ${name} lacks data-stick="${side}"`);
  }

  const polys = d.labels.map((l) => [l.anchor, ...l.via, l.labelAt]);
  const rects = d.labels.map((l) => chipRect(l.labelAt, l.side));
  const inRect = (r, [x, y], m = 0) => x > r.x - m && x < r.x + CHIP.w + m && y > r.y - m && y < r.y + CHIP.h + m;
  d.labels.forEach((l, i) => {
    const r = rects[i];
    if (r.x < VIEWBOX[0] || r.y < VIEWBOX[1] || r.x + CHIP.w > VIEWBOX[2] || r.y + CHIP.h > VIEWBOX[3]) problems.push(`${d.id}: chip ${l.control} leaves the viewBox`);
    for (let j = i + 1; j < d.labels.length; j++) {
      const q = rects[j];
      if (r.x < q.x + CHIP.w + 2 && q.x < r.x + CHIP.w + 2 && r.y < q.y + CHIP.h + 2 && q.y < r.y + CHIP.h + 2) problems.push(`${d.id}: chips ${l.control} and ${d.labels[j].control} overlap`);
      for (let a = 0; a < polys[i].length - 1; a++) {
        for (let b = 0; b < polys[j].length - 1; b++) {
          if (segmentsCross(polys[i][a], polys[i][a + 1], polys[j][b], polys[j][b + 1])) problems.push(`${d.id}: leaders ${l.control} and ${d.labels[j].control} cross`);
        }
      }
    }
    // leaders must not run through chips (their own chip only at its attach point)
    const pts = polys[i];
    for (let a = 0; a < pts.length - 1; a++) {
      const [p, q] = [pts[a], pts[a + 1]];
      const steps = Math.ceil(Math.hypot(q[0] - p[0], q[1] - p[1]));
      for (let k = 0; k <= steps; k++) {
        const t = k / steps;
        const s = [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
        rects.forEach((rr, j) => {
          const own = j === i;
          if (inRect(rr, s, own ? -1.5 : 2)) problems.push(`${d.id}: leader ${l.control} runs through chip ${d.labels[j].control}`);
        });
      }
    }
  });
  return [...new Set(problems)];
}

// ---------------------------------------------------------------- preview

function previewHtml(devices) {
  const data = devices.map((d) => ({
    id: d.id,
    title: d.title,
    file: d.file,
    svg: readFileSync(join(OUT, d.file), 'utf8'),
    controls: d.labels,
  }));
  const template = readFileSync(join(OUT, 'preview.template.html'), 'utf8');
  return template.replace('/*DEVICES*/[]', () => JSON.stringify(data).replace(/</g, '\\u003c')).replace('/*CHIP*/{}', JSON.stringify(CHIP));
}

/** layouts/index.ts: every layout by id, in the order of `devices`. */
function indexTs(devices) {
  const imports = [...devices.map((d) => [d.id, `import { ${d.id} } from './${d.id}';`]), ['types', "import type { DeviceLayout } from './types';"]]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([, line]) => line);
  return [
    '// All device drawings with their label layouts, by id. Generated by ../generate.mjs; do not',
    '// edit by hand.',
    '',
    ...imports,
    '',
    "export type { DeviceLayout, LayoutControl } from './types';",
    '',
    'export const DEVICE_LAYOUTS = {',
    ...devices.map((d) => `  ${d.id},`),
    '} satisfies Record<string, DeviceLayout>;',
    '',
    'export type DeviceId = keyof typeof DEVICE_LAYOUTS;',
    '',
  ].join('\n');
}

// ---------------------------------------------------------------- main

const devices = [
  dualsense(),
  dualsense({ edge: true }),
  xbox(),
  generic(),
  switchPro(),
  switch2Pro(),
  joyCons(),
  joyCons({ gen2: true }),
  gameCube(),
  dualsenseBack(),
  dualsenseBack({ edge: true }),
  xboxBack(),
  xboxBack({ elite: true }),
  genericBack(),
  switchProBack(),
  switchProBack({ gen2: true }),
  joyConsBack(),
  gameCubeBack(),
  mouse(),
];
let problems = [];
for (const d of devices) {
  writeFileSync(join(OUT, d.file), d.svg());
  writeFileSync(join(OUT, 'layouts', `${d.id}.ts`), d.layoutTs());
  problems = problems.concat(checkDevice(d, readFileSync(join(OUT, d.file), 'utf8')));
}
writeFileSync(join(OUT, 'layouts', 'index.ts'), indexTs(devices));
writeFileSync(join(OUT, 'preview.html'), previewHtml(devices));

for (const d of devices) console.log(`${d.file.padEnd(20)} ${String(d.controls.length).padStart(2)} controls`);
if (problems.length) {
  console.error(`\n${problems.length} problem(s):\n  ${problems.join('\n  ')}`);
  process.exit(1);
}
console.log('\nAll checks passed.');
