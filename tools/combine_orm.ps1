# Pack separate grayscale metal/roughness JPGs into Bevy's combined
# metallic-roughness texture (glTF ORM convention): R=ambient occlusion (255),
# G=roughness, B=metallic. Output PNG next to the source textures.
Add-Type -AssemblyName System.Drawing

$dir = "C:\Users\Benji\fabled\assets\textures\cyberpunk"

function Read-Gray($path) {
    $bmp = [System.Drawing.Bitmap]::FromFile($path)
    $w = $bmp.Width; $h = $bmp.Height
    $rect = New-Object System.Drawing.Rectangle 0, 0, $w, $h
    $data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $stride = $data.Stride
    $bytes = New-Object byte[] ($stride * $h)
    [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
    $bmp.UnlockBits($data); $bmp.Dispose()
    return @{ bytes = $bytes; stride = $stride; w = $w; h = $h }
}

function Combine($name, $hasMetal) {
    $rough = Read-Gray (Join-Path $dir "${name}_roughness.jpg")
    $w = $rough.w; $h = $rough.h
    $metal = if ($hasMetal) { Read-Gray (Join-Path $dir "${name}_metal.jpg") } else { $null }

    $dst = New-Object System.Drawing.Bitmap $w, $h, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $rect = New-Object System.Drawing.Rectangle 0, 0, $w, $h
    $dData = $dst.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::WriteOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $dStride = $dData.Stride
    $dBytes = New-Object byte[] ($dStride * $h)

    $rb = $rough.bytes; $rs = $rough.stride
    $mb = if ($metal) { $metal.bytes } else { $null }
    $ms = if ($metal) { $metal.stride } else { 0 }

    for ($y = 0; $y -lt $h; $y++) {
        $rRow = $y * $rs; $mRow = $y * $ms; $dRow = $y * $dStride
        for ($x = 0; $x -lt $w; $x++) {
            $roughVal = $rb[$rRow + $x * 3]              # grayscale: B=G=R
            $metalVal = if ($mb) { $mb[$mRow + $x * 3] } else { 0 }
            $o = $dRow + $x * 4
            $dBytes[$o]     = $metalVal   # B = metallic
            $dBytes[$o + 1] = $roughVal   # G = roughness
            $dBytes[$o + 2] = 255         # R = ambient occlusion (unused -> white)
            $dBytes[$o + 3] = 255         # A
        }
    }
    [System.Runtime.InteropServices.Marshal]::Copy($dBytes, 0, $dData.Scan0, $dBytes.Length)
    $dst.UnlockBits($dData)
    $out = Join-Path $dir "${name}_orm.png"
    $dst.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
    $dst.Dispose()
    Write-Host "wrote $out (${w}x${h}, metal=$hasMetal)"
}

Combine "wall" $true
Combine "floor" $false   # Concrete016 — non-metallic
Combine "ceiling" $false
Write-Host "done"
