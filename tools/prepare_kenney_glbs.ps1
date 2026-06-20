# Prepare Kenney Modular Space Kit GLBs for Bevy and Windows 3D Viewer.
#
# Every GLB (except gate-lasers*) references an EXTERNAL texture at:
#   Textures/colormap.png
# relative to the .glb file. Without that subfolder the mesh loads blank /
# fails entirely in Bevy, and Windows 3D Viewer shows an untextured or broken model.
#
# Run from repo root:
#   powershell -ExecutionPolicy Bypass -File tools/prepare_kenney_glbs.ps1

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

$space = "assets/models/space"
$texDir = Join-Path $space "Textures"
$colormap = Join-Path $texDir "colormap.png"
$srcColormap = Join-Path $space "colormap.png"

Write-Host "=== Kenney GLB prep ===" -ForegroundColor Cyan

# 1. Ensure shared atlas exists where every GLB expects it.
if (-not (Test-Path $texDir)) {
    New-Item -ItemType Directory -Path $texDir | Out-Null
}
if (-not (Test-Path $colormap)) {
    if (-not (Test-Path $srcColormap)) {
        throw "Missing $srcColormap - copy Kenney colormap.png into assets/models/space/"
    }
    Copy-Item $srcColormap $colormap
    Write-Host "Copied colormap -> Textures/colormap.png"
}

# Optional variation atlases (not referenced by GLBs, but kept for manual edits).
foreach ($v in @("variation-a.png", "variation-b.png")) {
    $dst = Join-Path $texDir $v
    $src = Join-Path $space $v
    if ((Test-Path $src) -and -not (Test-Path $dst)) {
        Copy-Item $src $dst
    }
}

# 2. Scan GLBs for texture URI.
$glbs = Get-ChildItem (Join-Path $space "*.glb") | Sort-Object Name
$missing = @()
$external = 0
$embedded = 0
foreach ($g in $glbs) {
    $bytes = [System.IO.File]::ReadAllBytes($g.FullName)
    $text = [System.Text.Encoding]::ASCII.GetString($bytes)
    if ($text -match 'Textures/colormap\.png') {
        $external++
    } elseif ($text -match 'colormap') {
        $embedded++
    } else {
        $missing += $g.Name
    }
}
Write-Host "GLBs: $($glbs.Count) total, $external external-texture, $embedded embedded-texture"
if ($missing.Count -gt 0) {
    Write-Warning "No colormap reference found in: $($missing -join ', ')"
}

# 3. Cyberpunk recolour atlas for in-game material swap.
Write-Host "Regenerating kenney_catalog.json ..."
& python (Join-Path $PSScriptRoot "generate_kenney_catalog.py")
if ($LASTEXITCODE -ne 0) { Write-Warning "generate_kenney_catalog.py failed" }

Write-Host "Regenerating cyber_colormap* ..."
& powershell -ExecutionPolicy Bypass -File tools/recolor_kenney_atlas.ps1

# 4. Self-contained viewer copies (Windows 3D Viewer cannot load external Textures/).
Write-Host "Embedding textures into assets/models/space/viewer/ ..."
$py = Get-Command python -ErrorAction SilentlyContinue
if ($null -eq $py) {
    Write-Warning "python not found - cannot build viewer copies."
    Write-Host "Open originals from $space only if Textures/ stays alongside (Bevy OK; 3D Viewer often fails)."
} else {
    & python (Join-Path $PSScriptRoot "embed_kenney_textures.py")
    if ($LASTEXITCODE -ne 0) { Write-Warning "embed_kenney_textures.py failed" }
}

Write-Host 'Done. Kenney mode: cargo run -- --host --test --kenney' -ForegroundColor Green
Write-Host 'Rusty mode:     cargo run -- --host --test' -ForegroundColor Green
