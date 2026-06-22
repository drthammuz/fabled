Add-Type -AssemblyName System.Drawing
$bmp = [System.Drawing.Bitmap]::FromFile("$PSScriptRoot\..\userinput\examplegrid.png")
Write-Host "Size: $($bmp.Width)x$($bmp.Height)"

function Classify($c) {
    if ($c.R -lt 80 -and $c.G -lt 80 -and $c.B -gt 150) { return 'B' }
    if ($c.G -gt 150 -and $c.R -lt 100 -and $c.B -lt 100) { return 'G' }
    if ($c.R -gt 150 -and $c.G -lt 100 -and $c.B -lt 100) { return 'R' }
    if ($c.R -gt 150 -and $c.G -gt 150 -and $c.B -lt 100) { return 'Y' }
    if ($c.R -lt 60 -and $c.G -lt 60 -and $c.B -lt 60) { return 'K' }
    return '.'
}

$minX = 999; $maxX = 0; $minY = 999; $maxY = 0
for ($y = 0; $y -lt $bmp.Height; $y++) {
    for ($x = 0; $x -lt $bmp.Width; $x++) {
        $ch = Classify $bmp.GetPixel($x, $y)
        if ($ch -in @('B','G','R','Y')) {
            if ($x -lt $minX) { $minX = $x }
            if ($x -gt $maxX) { $maxX = $x }
            if ($y -lt $minY) { $minY = $y }
            if ($y -gt $maxY) { $maxY = $y }
        }
    }
}
Write-Host "Content bounds: x=$minX..$maxX y=$minY..$maxY"

$cellW = ($maxX - $minX + 1) / 9.0
$cellH = ($maxY - $minY + 1) / 6.0
Write-Host "Cell size approx: $cellW x $cellH"

for ($row = 0; $row -lt 6; $row++) {
    $line = ""
    for ($col = 0; $col -lt 9; $col++) {
        $cx = [int]($minX + ($col + 0.5) * $cellW)
        $cy = [int]($minY + ($row + 0.5) * $cellH)
        $line += (Classify $bmp.GetPixel($cx, $cy))
    }
    Write-Host "cell row$row : $line"
}

Write-Host "--- H edges (north-south between rows, row0=north) ---"
for ($row = 0; $row -lt 5; $row++) {
    $line = ""
    for ($col = 0; $col -lt 9; $col++) {
        $cx = [int]($minX + ($col + 0.5) * $cellW)
        $cy = [int]($minY + ($row + 1) * $cellH - 3)
        $line += (Classify $bmp.GetPixel($cx, $cy))
    }
    Write-Host "H$row-$($row+1): $line"
}

Write-Host "--- V edges (west-east between cols) ---"
for ($col = 0; $col -lt 8; $col++) {
    $line = ""
    for ($row = 0; $row -lt 6; $row++) {
        $cx = [int]($minX + ($col + 1) * $cellW - 3)
        $cy = [int]($minY + ($row + 0.5) * $cellH)
        $line += (Classify $bmp.GetPixel($cx, $cy))
    }
    Write-Host "V$col-$($col+1) r0-5: $line"
}

$bmp.Dispose()
