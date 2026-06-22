Add-Type -AssemblyName System.Drawing
$bmp = [System.Drawing.Bitmap]::FromFile("$PSScriptRoot\..\userinput\examplegrid.png")

function Classify($c) {
    if ($c.B -gt 180 -and $c.R -lt 120 -and $c.G -lt 120) { return 'B' }
    if ($c.G -gt 180 -and $c.R -lt 120 -and $c.B -lt 120) { return 'G' }
    if ($c.R -gt 180 -and $c.G -lt 120 -and $c.B -lt 120) { return 'R' }
    if ($c.R -gt 180 -and $c.G -gt 180 -and $c.B -lt 120) { return 'Y' }
    if ($c.R -lt 80 -and $c.G -lt 80 -and $c.B -lt 80) { return 'K' }
    return '.'
}

$gridX = @(5, 48, 91, 134, 177, 220, 263, 306, 349, 390)
$gridY = @(6, 52, 98, 145, 191, 237, 284)

Write-Host "Grid 9x6 - sampling ON each grid line (G=wall R=door B=border Y=stairs K=black)"
Write-Host ""

Write-Host "HORIZONTAL lines (wall_ns candidates) - scan along x for each grid Y index:"
for ($gi = 0; $gi -lt $gridY.Count; $gi++) {
    $y = $gridY[$gi]
    $seg = ""
    for ($ci = 0; $ci -lt ($gridX.Count-1); $ci++) {
        $x0 = $gridX[$ci] + 2
        $x1 = $gridX[$ci+1] - 2
        $found = '.'
        for ($x = $x0; $x -le $x1; $x++) {
            $ch = Classify $bmp.GetPixel($x, $y)
            if ($ch -ne '.') { $found = $ch; break }
        }
        $seg += $found
    }
    Write-Host ("  y[{0}]={1,3} cols0-8: {2}" -f $gi, $y, $seg)
}

Write-Host ""
Write-Host "VERTICAL lines (wall_ew candidates) - scan along y for each grid X index:"
for ($gi = 0; $gi -lt $gridX.Count; $gi++) {
    $x = $gridX[$gi]
    $seg = ""
    for ($ri = 0; $ri -lt ($gridY.Count-1); $ri++) {
        $y0 = $gridY[$ri] + 2
        $y1 = $gridY[$ri+1] - 2
        $found = '.'
        for ($y = $y0; $y -le $y1; $y++) {
            $ch = Classify $bmp.GetPixel($x, $y)
            if ($ch -ne '.') { $found = $ch; break }
        }
        $seg += $found
    }
    Write-Host ("  x[{0}]={1,3} rows0-5: {2}" -f $gi, $x, $seg)
}

Write-Host ""
Write-Host "Cell centers (row0=north):"
for ($ri = 0; $ri -lt 6; $ri++) {
    $line = ""
    for ($ci = 0; $ci -lt 9; $ci++) {
        $x = [int](($gridX[$ci]+$gridX[$ci+1])/2)
        $y = [int](($gridY[$ri]+$gridY[$ri+1])/2)
        $line += (Classify $bmp.GetPixel($x,$y))
    }
    Write-Host "  row$ri : $line"
}

$bmp.Dispose()
