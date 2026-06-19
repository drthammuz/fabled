# Generate a grid image from userinput/wall_map.json (single source of truth).
# Usage: powershell -File tools/generate_wall_grid.ps1
Add-Type -AssemblyName System.Drawing

$jsonPath = Join-Path $PSScriptRoot "..\userinput\wall_map.json"
$outPath  = Join-Path $PSScriptRoot "..\userinput\generated_wall_grid.png"
$verifyPath = Join-Path $PSScriptRoot "..\userinput\wall_map_verify.txt"
$spec     = Get-Content $jsonPath -Raw | ConvertFrom-Json

$cellPx = 48
$margin = 52
$maxX   = [int]$spec.grid.max_x
$maxY   = [int]$spec.grid.max_y
$wallPx = 14

$imgW = $margin * 2 + $maxX * $cellPx
$imgH = $margin * 2 + $maxY * $cellPx

$bmp = New-Object System.Drawing.Bitmap $imgW, $imgH
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::None
$g.Clear([System.Drawing.Color]::FromArgb(245, 245, 245))

function Px([double]$x) { return [int]($margin + $x * $cellPx) }
function Py([double]$y) { return [int]($margin + ($maxY - $y) * $cellPx) }

$penGrid = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(30, 30, 30)), 1
$brushBlue  = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(30, 80, 220))
$brushGreen = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(30, 180, 60))
$brushRed   = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(220, 50, 50))
$brushYellow = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(230, 210, 40))
$fontSmall = New-Object System.Drawing.Font "Consolas", 8
$fontLabel = New-Object System.Drawing.Font "Consolas", 7

for ($x = 0; $x -le $maxX; $x++) {
    $px = Px $x
    $g.DrawLine($penGrid, $px, (Py 0), $px, (Py $maxY))
}
for ($y = 0; $y -le $maxY; $y++) {
    $py = Py $y
    $g.DrawLine($penGrid, (Px 0), $py, (Px $maxX), $py)
}

# Corner labels — proves coordinate placement
for ($x = 0; $x -le $maxX; $x++) {
    for ($y = 0; $y -le $maxY; $y++) {
        $label = "$x,$y"
        $g.DrawString($label, $fontLabel, [System.Drawing.Brushes]::DimGray, (Px $x) + 2, (Py $y) + 2)
    }
}

# Draw one wall segment between grid intersections (endpoints inclusive).
# $OpensAlong = door centres on segment axis (grid x for horizontal, grid y for vertical).
function DrawWallSeg {
    param(
        [Parameter(Mandatory)][double]$X0,
        [Parameter(Mandatory)][double]$Y0,
        [Parameter(Mandatory)][double]$X1,
        [Parameter(Mandatory)][double]$Y1,
        [Parameter(Mandatory)]$Brush,
        [double[]]$OpensAlong = @()
    )
    $doorHalf = 0.5
    if ([Math]::Abs($Y0 - $Y1) -lt 0.001) {
        $y = $Y0
        $xLo = [Math]::Min($X0, $X1)
        $xHi = [Math]::Max($X0, $X1)
        $ops = @($OpensAlong | Sort-Object)
        if ($ops.Count -eq 0) {
            $left  = Px $xLo
            $right = Px $xHi
            $py = Py $y
            $g.FillRectangle($Brush, $left, $py - [int]($wallPx / 2), ($right - $left) + 1, $wallPx)
            return
        }
        $cur = $xLo
        foreach ($ox in $ops) {
            if ($ox - $doorHalf -gt $cur + 0.001) {
                DrawWallSeg -X0 $cur -Y0 $y -X1 ($ox - $doorHalf) -Y1 $y -Brush $Brush
            }
            $cur = $ox + $doorHalf
        }
        if ($xHi -gt $cur + 0.001) {
            DrawWallSeg -X0 $cur -Y0 $y -X1 $xHi -Y1 $y -Brush $Brush
        }
    } elseif ([Math]::Abs($X0 - $X1) -lt 0.001) {
        $x = $X0
        $yLo = [Math]::Min($Y0, $Y1)
        $yHi = [Math]::Max($Y0, $Y1)
        $ops = @($OpensAlong | Sort-Object)
        if ($ops.Count -eq 0) {
            $px = Px $x
            $top = Py $yHi
            $bot = Py $yLo
            $g.FillRectangle($Brush, $px - [int]($wallPx / 2), $top, $wallPx, ($bot - $top) + 1)
            return
        }
        $cur = $yLo
        foreach ($oy in $ops) {
            if ($oy - $doorHalf -gt $cur + 0.001) {
                DrawWallSeg -X0 $x -Y0 $cur -X1 $x -Y1 ($oy - $doorHalf) -Brush $Brush
            }
            $cur = $oy + $doorHalf
        }
        if ($yHi -gt $cur + 0.001) {
            DrawWallSeg -X0 $x -Y0 $cur -X1 $x -Y1 $yHi -Brush $Brush
        }
    } else {
        throw "Diagonal wall not supported: ($X0,$Y0)-($X1,$Y1)"
    }
}

function DoorOpeningOnWall($d, $w) {
    $dx0 = [double]$d.from[0]; $dy0 = [double]$d.from[1]
    $dx1 = [double]$d.to[0];   $dy1 = [double]$d.to[1]
    $wx0 = [double]$w.from[0]; $wy0 = [double]$w.from[1]
    $wx1 = [double]$w.to[0];  $wy1 = [double]$w.to[1]
    if ([Math]::Abs($wy0 - $wy1) -lt 0.001 -and [Math]::Abs($dy0 - $dy1) -lt 0.001 -and [Math]::Abs($wy0 - $dy0) -lt 0.001) {
        $wLo = [Math]::Min($wx0, $wx1); $wHi = [Math]::Max($wx0, $wx1)
        $dLo = [Math]::Min($dx0, $dx1); $dHi = [Math]::Max($dx0, $dx1)
        $oLo = [Math]::Max($wLo, $dLo); $oHi = [Math]::Min($wHi, $dHi)
        if ($oHi -gt $oLo + 0.001) { return ($oLo + $oHi) / 2.0 }
        $cx = ($dx0 + $dx1) / 2.0
        if ($cx -ge $wLo - 0.001 -and $cx -le $wHi + 0.001) { return $cx }
    }
    if ([Math]::Abs($wx0 - $wx1) -lt 0.001 -and [Math]::Abs($dx0 - $dx1) -lt 0.001 -and [Math]::Abs($wx0 - $dx0) -lt 0.001) {
        $wLo = [Math]::Min($wy0, $wy1); $wHi = [Math]::Max($wy0, $wy1)
        $dLo = [Math]::Min($dy0, $dy1); $dHi = [Math]::Max($dy0, $dy1)
        $oLo = [Math]::Max($wLo, $dLo); $oHi = [Math]::Min($wHi, $dHi)
        if ($oHi -gt $oLo + 0.001) { return ($oLo + $oHi) / 2.0 }
        $cy = ($dy0 + $dy1) / 2.0
        if ($cy -ge $wLo - 0.001 -and $cy -le $wHi + 0.001) { return $cy }
    }
    return $null
}

function DoorsOnSegment($wallsDoors, $w) {
    $along = @()
    foreach ($d in $wallsDoors) {
        $open = DoorOpeningOnWall $d $w
        if ($null -ne $open) { $along += $open }
    }
    return $along
}

function DrawDoorSeg($d) {
    DrawWallSeg -X0 $d.from[0] -Y0 $d.from[1] -X1 $d.to[0] -Y1 $d.to[1] -Brush $brushRed
}

$allDoors = @()
if ($spec.doors) { $allDoors = @($spec.doors) }

$verify = New-Object System.Collections.Generic.List[string]
$verify.Add("wall_map.json -> generated_wall_grid.png")
$verify.Add("")

foreach ($w in $spec.outer_walls) {
    $ops = DoorsOnSegment $allDoors $w
    DrawWallSeg -X0 $w.from[0] -Y0 $w.from[1] -X1 $w.to[0] -Y1 $w.to[1] -Brush $brushBlue -OpensAlong $ops
}
foreach ($w in $spec.inner_walls) {
    $ops = DoorsOnSegment $allDoors $w
    DrawWallSeg -X0 $w.from[0] -Y0 $w.from[1] -X1 $w.to[0] -Y1 $w.to[1] -Brush $brushGreen -OpensAlong $ops
    $verify.Add(("inner: ({0},{1})-({2},{3})" -f $w.from[0], $w.from[1], $w.to[0], $w.to[1]))
}
foreach ($d in $allDoors) {
    DrawDoorSeg $d
    $verify.Add(("door ({0},{1})-({2},{3})" -f $d.from[0], $d.from[1], $d.to[0], $d.to[1]))
}

# Stairs
$st = $spec.stairs.corners
$xs = @($st[0][0], $st[1][0], $st[2][0], $st[3][0])
$ys = @($st[0][1], $st[1][1], $st[2][1], $st[3][1])
$stX0 = ($xs | Measure-Object -Minimum).Minimum
$stX1 = ($xs | Measure-Object -Maximum).Maximum
$stY0 = ($ys | Measure-Object -Minimum).Minimum
$stY1 = ($ys | Measure-Object -Maximum).Maximum
$g.FillRectangle($brushYellow, (Px $stX0), (Py $stY1), (($stX1 - $stX0) * $cellPx), (($stY1 - $stY0) * $cellPx))
if ($spec.stairs.climb) {
    $cf = $spec.stairs.climb.from; $ct = $spec.stairs.climb.to
} else {
    $cf = @([double](($stX0 + $stX1) * 0.5), [double]$stY0)
    $ct = @([double](($stX0 + $stX1) * 0.5), [double]$stY1)
}
$ax0 = Px $cf[0]; $ay0 = Py $cf[1]
$ax1 = Px $ct[0]; $ay1 = Py $ct[1]
$penArrow = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(40, 40, 40)), 3
$g.DrawLine($penArrow, $ax0, $ay0, $ax1, $ay1)

$font = New-Object System.Drawing.Font "Segoe UI", 9
$g.DrawString("from wall_map.json  Blue=outer  Green=inner  Red=door  Yellow=stairs  (0,0)=SW  y up=N", $font,
    [System.Drawing.Brushes]::Black, 8, 8)

$bmp.Save($outPath, [System.Drawing.Imaging.ImageFormat]::Png)

function IsInnerGreen($c) { return $c.G -gt 120 -and $c.R -lt 100 -and $c.B -lt 100 }

# Verify each unit step along every inner segment
$verify.Add("")
$verify.Add("unit-edge pixel checks (midpoint of each 1-unit step):")
foreach ($w in $spec.inner_walls) {
    $x0 = [double]$w.from[0]; $y0 = [double]$w.from[1]
    $x1 = [double]$w.to[0]; $y1 = [double]$w.to[1]
    if ([Math]::Abs($x0 - $x1) -lt 0.001) {
        $x = $x0
        $ya = [int][Math]::Min($y0, $y1)
        $yb = [int][Math]::Max($y0, $y1)
        for ($y = $ya; $y -lt $yb; $y++) {
            $mx = Px $x
            $my = Py ($y + 0.5)
            $ok = IsInnerGreen ($bmp.GetPixel($mx, $my))
            $line = ("  ({0},{1})-({0},{2}): {3}" -f $x, $y, ($y + 1), $(if ($ok) { "OK" } else { "MISSING at px=$mx,$my" }))
            $verify.Add($line)
        }
    } elseif ([Math]::Abs($y0 - $y1) -lt 0.001) {
        $y = $y0
        $xa = [int][Math]::Min($x0, $x1)
        $xb = [int][Math]::Max($x0, $x1)
        for ($x = $xa; $x -lt $xb; $x++) {
            $mx = Px ($x + 0.5)
            $my = Py $y
            $ok = IsInnerGreen ($bmp.GetPixel($mx, $my))
            $line = ("  ({0},{2})-({1},{2}): {3}" -f $x, ($x + 1), $y, $(if ($ok) { "OK" } else { "MISSING at px=$mx,$my" }))
            $verify.Add($line)
        }
    }
}

$verify | Out-File -FilePath $verifyPath -Encoding utf8
$g.Dispose(); $bmp.Dispose()
Write-Host "Wrote $outPath"
Write-Host "Wrote $verifyPath"
Get-Content $verifyPath | Select-String "3,0|3,1|3,2|3,3|3,4"
