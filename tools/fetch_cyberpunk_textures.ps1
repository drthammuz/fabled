# Fetches CC0 industrial PBR textures (ambientCG stand-ins) and optional
# Industrial Cyberpunk Tilepack previews from itch.io.
# Full tilepack ORM zip: download manually from
# https://slipperhat.itch.io/industrial-cyberpunk-3d-tilepack (CC0)
# and extract into assets/textures/cyberpunk/tilepack/

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$dst = Join-Path $root "assets\textures\cyberpunk"
$dl = Join-Path $root "assets\_dl"
New-Item -ItemType Directory -Force -Path $dst, $dl | Out-Null

$packs = @(
    @{ Name = "MetalPlates008"; Prefix = "floor" },
    # Walls: weathered/rusted steel plate (old industrial metal look).
    @{ Name = "MetalPlates013"; Prefix = "wall" },
    @{ Name = "Concrete031"; Prefix = "ceiling" }
)

foreach ($p in $packs) {
    $zip = Join-Path $dl "$($p.Name).zip"
    curl.exe -L "https://ambientcg.com/get?file=$($p.Name)_1K-JPG.zip" -o $zip
    $folder = Join-Path $dl $p.Name
    Expand-Archive -Path $zip -DestinationPath $folder -Force
    Copy-Item (Join-Path $folder "$($p.Name)_1K-JPG_Color.jpg") (Join-Path $dst "$($p.Prefix)_color.jpg")
    Copy-Item (Join-Path $folder "$($p.Name)_1K-JPG_NormalGL.jpg") (Join-Path $dst "$($p.Prefix)_normal.jpg")
    Copy-Item (Join-Path $folder "$($p.Name)_1K-JPG_Roughness.jpg") (Join-Path $dst "$($p.Prefix)_roughness.jpg")
    if (Test-Path (Join-Path $folder "$($p.Name)_1K-JPG_Metalness.jpg")) {
        Copy-Item (Join-Path $folder "$($p.Name)_1K-JPG_Metalness.jpg") (Join-Path $dst "$($p.Prefix)_metal.jpg")
    }
}

curl.exe -L "https://img.itch.zone/aW1hZ2UvMjk3NDUyMi8xNzc5MjM5MS5wbmc=/original/VG1%2BIR.png" -o (Join-Path $dst "tilepack_preview_wall.png")
curl.exe -L "https://img.itch.zone/aW1hZ2UvMjk3NDUyMi8xNzc5MjA2Ny5wbmc=/original/qSdtTA.png" -o (Join-Path $dst "tilepack_preview_floor.png")

Write-Host "Textures ready in $dst"
