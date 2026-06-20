# Recolours the Kenney Modular Space Kit colour atlas into a cyberpunk palette
# with a worn/aged surface filter (grain, grime, rare scratches).
#
# Run:  powershell -ExecutionPolicy Bypass -File tools/recolor_kenney_atlas.ps1

Add-Type -AssemblyName System.Drawing

$src    = "assets/models/space/colormap.png"
$dstB   = "assets/models/space/cyber_colormap.png"
$dstMr  = "assets/models/space/cyber_colormap_mr.png"
$dstEmi = "assets/models/space/cyber_colormap_emissive.png"

# ── HSL -> int[3] RGB ─────────────────────────────────────────────────────────
function HslToRgb([double]$h, [double]$s, [double]$l) {
    if ($s -le 0.0) { [int]$v = [math]::Round($l*255); return @($v,$v,$v) }
    [double]$c  = (1.0 - [math]::Abs(2.0*$l-1.0)) * $s
    [double]$hp = $h / 60.0
    [double]$x  = $c * (1.0 - [math]::Abs(($hp % 2.0) - 1.0))
    [double]$r1=0; [double]$g1=0; [double]$b1=0
    switch ([int][math]::Floor($hp)) {
        0 { $r1=$c; $g1=$x } 1 { $r1=$x; $g1=$c }
        2 { $g1=$c; $b1=$x } 3 { $g1=$x; $b1=$c }
        4 { $r1=$x; $b1=$c } default { $r1=$c; $b1=$x }
    }
    [double]$m = $l - $c/2.0
    [int]$ri = [math]::Round([math]::Max(0.0,[math]::Min(1.0,$r1+$m))*255)
    [int]$gi = [math]::Round([math]::Max(0.0,[math]::Min(1.0,$g1+$m))*255)
    [int]$bi = [math]::Round([math]::Max(0.0,[math]::Min(1.0,$b1+$m))*255)
    return @($ri,$gi,$bi)
}

# ── Deterministic per-pixel pseudo-noise [0,1) ───────────────────────────────
# sin-based hash — no integer overflow risk, fast enough for a 512² atlas.
function PixNoise([int]$px, [int]$py) {
    [double]$v = [math]::Sin([double]$px * 127.1 + [double]$py * 311.7) * 43758.5453
    return $v - [math]::Floor($v)
}

# ── Load ─────────────────────────────────────────────────────────────────────
$bmp = New-Object System.Drawing.Bitmap (Resolve-Path $src).Path
[int]$W = $bmp.Width; [int]$H = $bmp.Height
Write-Host "source atlas: $W x $H"

$rect = New-Object System.Drawing.Rectangle 0, 0, $W, $H
$fmt  = [System.Drawing.Imaging.PixelFormat]::Format32bppArgb

$srcD   = $bmp.LockBits($rect,[System.Drawing.Imaging.ImageLockMode]::ReadOnly,$fmt)
[int]$stride = $srcD.Stride
[int]$len = [math]::Abs($stride) * $H
$srcBuf = New-Object byte[] $len
[System.Runtime.InteropServices.Marshal]::Copy($srcD.Scan0,$srcBuf,0,$len)
$bmp.UnlockBits($srcD)
$bmp.Dispose()

$baseBuf = New-Object byte[] $len
$mrBuf   = New-Object byte[] $len
$emiBuf  = New-Object byte[] $len

# ── Per-pixel remap + worn filter ────────────────────────────────────────────
for ([int]$py = 0; $py -lt $H; $py++) {
    [int]$row = $py * $stride
    for ([int]$px = 0; $px -lt $W; $px++) {
        [int]$i = $row + $px * 4

        [int]$sb = $srcBuf[$i]; [int]$sg = $srcBuf[$i+1]
        [int]$sr = $srcBuf[$i+2]; [int]$sa = $srcBuf[$i+3]

        $col = [System.Drawing.Color]::FromArgb($sa,$sr,$sg,$sb)
        [double]$sh = $col.GetHue()
        [double]$ss = $col.GetSaturation()
        [double]$sl = $col.GetBrightness()

        # ── Colour remap ──
        [double]$nh=0; [double]$ns=0; [double]$nl=0
        [int]$metallic=0; [int]$rough=0
        [int]$er=0; [int]$eg=0; [int]$eb=0

        if ($ss -lt 0.18 -or ($sh -ge 185 -and $sh -lt 260 -and $ss -lt 0.28)) {
            # Structural neutral + lightly-blue structural greys (stair risers) → gunmetal
            $nh=210.0; $ns=0.10; $nl=0.04+$sl*0.32
            $metallic=230; $rough=[int](100+(1.0-$sl)*80)
        } elseif ($sh -ge 20 -and $sh -lt 65) {
            # Warm (yellow/orange) → Fallout-pale matte clay. No emissive — the
            # worn filter already adds enough variation; glowing yellow looks garish.
            $nh=36.0; $ns=0.28; $nl=0.09+$sl*0.07
            $metallic=30; $rough=210
        } elseif ($sh -ge 65 -and $sh -lt 175) {
            # Greens → toxic neon green
            $nh=145.0; $ns=0.70; $nl=0.22+$sl*0.28
            $metallic=40; $rough=90
            if ($sl -gt 0.55 -and $ss -gt 0.45) {
                $e=HslToRgb 145.0 0.85 0.38; $er=$e[0]; $eg=$e[1]; $eb=$e[2]
            }
        } elseif ($sh -ge 175 -and $sh -lt 255) {
            # Blues → deep steel / cyan
            $nh=198.0; $ns=0.55; $nl=0.20+$sl*0.34
            $metallic=220; $rough=85
            if ($sl -gt 0.55 -and $ss -gt 0.45) {
                $e=HslToRgb 198.0 0.80 0.40; $er=$e[0]; $eg=$e[1]; $eb=$e[2]
            }
        } else {
            # Reds/pinks/purples → danger magenta
            $nh=305.0; $ns=0.55; $nl=0.22+$sl*0.26
            $metallic=30; $rough=100
            if ($sl -gt 0.55 -and $ss -gt 0.45) {
                $e=HslToRgb 305.0 0.80 0.40; $er=$e[0]; $eg=$e[1]; $eb=$e[2]
            }
        }

        $rgb=HslToRgb $nh $ns $nl
        [int]$br=$rgb[0]; [int]$bg=$rgb[1]; [int]$bb=$rgb[2]

        # ── Worn / aged filter ────────────────────────────────────────────────
        # 1. Fine grain noise: subtle per-pixel brightness jitter (±8 DN)
        [double]$n1 = PixNoise $px $py
        [int]$grain = [int](($n1-0.5)*16.0)

        # 2. Low-freq grime blotches: darker patches simulate settled dust/grease
        [int]$gx=[int]($px/7); [int]$gy=[int]($py/7)
        [double]$n2 = PixNoise $gx $gy
        [int]$grime = [int]($n2*22.0)   # 0..22 DN darkening only

        # 3. Rare scratch pixels (~1% chance): a sharp dark slash
        [double]$n3 = PixNoise ($px*7+13) ($py*3+7)
        [int]$scratch = if ($n3 -lt 0.012) { -55 } else { 0 }

        $br=[math]::Max(0,[math]::Min(255,$br+$grain-$grime+$scratch))
        $bg=[math]::Max(0,[math]::Min(255,$bg+$grain-$grime+$scratch))
        $bb=[math]::Max(0,[math]::Min(255,$bb+$grain-$grime+$scratch))

        # ── Write outputs ──
        # System.Drawing BGRA order
        $baseBuf[$i]   = [byte]$bb; $baseBuf[$i+1]=[byte]$bg
        $baseBuf[$i+2] = [byte]$br; $baseBuf[$i+3]=[byte]$sa

        # MR map (glTF: G=roughness, B=metallic)
        $mrBuf[$i]   = [byte]$metallic; $mrBuf[$i+1]=[byte]$rough
        $mrBuf[$i+2] = 0;               $mrBuf[$i+3]=255

        # Emissive
        $emiBuf[$i]   = [byte]$eb; $emiBuf[$i+1]=[byte]$eg
        $emiBuf[$i+2] = [byte]$er; $emiBuf[$i+3]=255
    }
}

# ── Save ─────────────────────────────────────────────────────────────────────
function SaveBuf($buf, $path) {
    $out = New-Object System.Drawing.Bitmap $W, $H, $fmt
    $d = $out.LockBits($rect,[System.Drawing.Imaging.ImageLockMode]::WriteOnly,$fmt)
    [System.Runtime.InteropServices.Marshal]::Copy($buf,0,$d.Scan0,$buf.Length)
    $out.UnlockBits($d)
    $out.Save((Join-Path (Get-Location) $path),[System.Drawing.Imaging.ImageFormat]::Png)
    $out.Dispose(); Write-Host "wrote $path"
}

SaveBuf $baseBuf $dstB
SaveBuf $mrBuf   $dstMr
SaveBuf $emiBuf  $dstEmi
Write-Host "done."
