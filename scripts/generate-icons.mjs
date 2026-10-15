// Regenerates every derived icon in src-tauri/icons from the Icon Composer
// document src-tauri/icons/diffract.icon (the single source of truth), and
// the amber "Diffract Dev" set in src-tauri/icons/dev from dev/diffract-dev.icon
// (used by tauri.dev.conf.json). `node scripts/generate-icons.mjs dev` builds
// only the dev set; `main` only the release one.
//
// - Assets.car: compiled by Apple's actool. Add it back to `bundle.icon` in
//   tauri.conf.json and tauri.dev.conf.json when you compile it; this fork ships
//   the raster `.icns` + PNGs instead, because compiling an Icon Composer
//   document needs Xcode (actool 26 and Icon Composer's ictool) and the dev
//   machine this fork was set up on has only CommandLineTools. Tauri could
//   compile a `.icon` itself, but its in-process actool call crashes:
//   https://github.com/tauri-apps/tauri/issues/15315.
// - icon.icns + PNGs: rendered by Icon Composer's ictool, then run through
//   `tauri icon`. icon.icns is CFBundleIconFile, the fallback macOS uses when
//   there is no Assets.car. `icon.ico` is not generated: this fork has no
//   Windows target.
//
// Requires Xcode 26+ (actool 26 and Icon Composer's `ictool`).
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const rootDir = join(dirname(fileURLToPath(import.meta.url)), '..');
const variants = {
  main: { iconsDir: join(rootDir, 'src-tauri', 'icons'), name: 'diffract' },
  dev: { iconsDir: join(rootDir, 'src-tauri', 'icons', 'dev'), name: 'diffract-dev' },
};
const only = process.argv[2];
if (only && !variants[only]) {
  console.error(`Unknown icon set "${only}". Use one of: ${Object.keys(variants).join(', ')}`);
  process.exit(1);
}

const developerDir = execFileSync('xcode-select', ['-p'], { encoding: 'utf8' }).trim();
const ictool = join(
  developerDir,
  '..',
  'Applications',
  'Icon Composer.app',
  'Contents',
  'Executables',
  'ictool',
);

// macOS icon grid: the icon shape is 824pt inside a 1024pt canvas.
const canvasSize = 1024;
const shapeSize = 824;

// Keep in sync with bundle.macOS.minimumSystemVersion in tauri.conf.json.
const minimumSystemVersion = '13.3';

function generate({ iconsDir, name }) {
  const iconDocument = join(iconsDir, `${name}.icon`);
  const workDir = mkdtempSync(join(tmpdir(), 'diffity-icons-'));

  try {
    const shapePng = join(workDir, 'shape.png');
    const sourceSvg = join(workDir, 'app-icon.svg');
    const tauriOut = join(workDir, 'tauri');
    const catalogOut = join(workDir, 'catalog');
    mkdirSync(catalogOut);

    execFileSync('xcrun', [
      'actool', iconDocument,
      '--compile', catalogOut,
      '--output-format', 'human-readable-text',
      '--notices',
      '--warnings',
      '--output-partial-info-plist', join(catalogOut, 'partial-info.plist'),
      '--app-icon', name,
      '--include-all-app-icons',
      '--enable-on-demand-resources', 'NO',
      '--development-region', 'en',
      '--target-device', 'mac',
      '--platform', 'macosx',
      '--minimum-deployment-target', minimumSystemVersion,
    ], { stdio: 'inherit' });
    copyFileSync(join(catalogOut, 'Assets.car'), join(iconsDir, 'Assets.car'));

    execFileSync(ictool, [
      iconDocument,
      '--export-image',
      '--output-file', shapePng,
      '--platform', 'macOS',
      '--rendition', 'Default',
      '--width', String(shapeSize),
      '--height', String(shapeSize),
      '--scale', '1',
    ], { stdio: 'inherit' });

    // Center the shape on the transparent canvas. `tauri icon` rasterizes SVG
    // sources, so wrapping the PNG in an SVG keeps this dependency-free.
    const inset = (canvasSize - shapeSize) / 2;
    const shapeData = readFileSync(shapePng).toString('base64');
    writeFileSync(
      sourceSvg,
      `<svg xmlns="http://www.w3.org/2000/svg" width="${canvasSize}" height="${canvasSize}" viewBox="0 0 ${canvasSize} ${canvasSize}">` +
        `<image x="${inset}" y="${inset}" width="${shapeSize}" height="${shapeSize}" href="data:image/png;base64,${shapeData}"/>` +
        '</svg>',
    );

    execFileSync('pnpm', ['tauri', 'icon', sourceSvg, '--output', tauriOut], {
      cwd: rootDir,
      stdio: 'inherit',
    });

    for (const file of [
      '32x32.png',
      '64x64.png',
      '128x128.png',
      '128x128@2x.png',
      'icon.png',
      'icon.icns',
    ]) {
      copyFileSync(join(tauriOut, file), join(iconsDir, file));
    }
  } finally {
    rmSync(workDir, { recursive: true, force: true });
  }
}

for (const [key, variant] of Object.entries(variants)) {
  if (!only || only === key) {
    generate(variant);
  }
}
