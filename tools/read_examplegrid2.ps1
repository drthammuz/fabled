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

# Find vertical blue lines (module col boundaries)
$blueCols = @()
for ($x = 0; $x -lt $bmp.Width; $x++) {
    $cnt = 0
    for ($y = 0; $y -lt $bmp.Height; $y++) {
        if ((Classify $bmp.GetPixel($x,$y)) -eq 'B') { $cnt++ }
    }
    if ($cnt -gt 40) { $blueCols += $x }
}
# cluster
$vLines = @()
$last = -99
foreach ($x in ($blueCols | Sort-Object)) {
    if ($x - $last -gt 5) { $vLines += $x }
    $last = $x
}

# Find horizontal blue lines
$blueRows = @()
for ($y = 0; $y -lt $bmp.Height; $y++) {
    $cnt = 0
    for ($x = 0; $x -lt $bmp.Width; $x++) {
        if ((Classify $bmp.GetPixel($x,$y)) -eq 'B') { $cnt++ }
    }
    if ($cnt -gt 40) { $blueRows += $y }
}
$hLines = @()
$last = -99
foreach ($y in ($blueRows | Sort-Object)) {
    if ($y - $last -gt 5) { $hLines += $y }
}

Write-Host "V blue lines: $($vLines -join ', ')"
Write-Host "H blue lines: $($hLines -join ', ')"

# inner grid lines = midpoints between module borders + thin black lines
# 9 cols => 10 vertical grid lines including outer; module borders at cols 0,3,6,9
# Derive all 10 vertical grid x positions by subdividing each module into 3
$modXL = @($vLines[0], $vLines[2], $vLines[4], $vLines[6])  # 4 module west edges + east outer
if ($vLines.Count -ge 7) {
    $x0=$vLines[0]; $x1=$vLines[2]; $x2=$vLines[4]; $x3=$vLines[6]
    $gridX = @()
    foreach ($pair in @(@($x0,$x1),@($x1,$x2),@($x2,$x3))) {
        $a=$pair[0]; $b=$pair[1]
        for ($i=0; $i -lt 3; $i++) { $gridX += [int]($a + ($b-$a)*$i/3.0) }
    }
    $gridX += $x3
    Write-Host "Grid X lines: $($gridX -join ', ')"
}

$y0=$hLines[0]; $y1=$hLines[2]; $y2=$hLines[4]
$gridY = @()
foreach ($pair in @(@($y0,$y1),@($y1,$y2))) {
    $a=$pair[0]; $b=$pair[1]
    for ($i=0; $i -lt 3; $i++) { $gridY += [int]($a + ($b-$a)*$i/3.0) }
}
$gridY += $y2
Write-Host "Grid Y lines: $($gridY -join ', ')"

function SampleEdge($x1,$y1,$x2,$y2) {
    $steps = [Math]::Max([Math]::Abs($x2-$x1), [Math]::Abs($y2-$y1))
    if ($steps -lt 1) { $steps = 1 }
    $chars = ''
    for ($i=0; $i -le $steps; $i++) {
        $t = $i / $steps
        $x = [int]($x1 + ($x2-$x1)*$t)
        $y = [int]($y1 + ($y2-$y1)*$t)
        $chars += (Classify $bmp.GetPixel($x,$y))
    }
    # collapse to dominant non-dot non-K
    foreach ($ch in @('R','G','Y','B')) {
        if ($chars.Contains($ch)) { return $ch }
    }
    if ($chars.Contains('K')) { return 'K' }
    return '.'
}

Write-Host "`n--- Horizontal internal edges (between rows) ---"
for ($r = 0; $r -lt ($gridY.Count-1); $r++) {
    $y = [int](($gridY[$r] + $gridY[$r+1]) / 2.0)
    if ($r -eq 0) { $y = $gridY[$r] + 8 }
    elseif ($r -eq $gridY.Count-2) { $y = $gridY[$r+1] - 8 }
    else { $y = [int](($gridY[$r] + $gridY[$r+1]) / 2.0) }
    $line = ''
    for ($c = 0; $c -lt ($gridX.Count-1); $c++) {
        $x = [int](($gridX[$c] + $gridX[$c+1]) / 2.0)
        $ch = Classify $bmp.GetPixel($x, $y)
        $line += $ch
    }
    Write-Host "between row$r and $($r+1) at y=$y : $line"
}

Write-Host "`n--- Vertical internal edges (between cols) ---"
for ($c = 0; $c -lt ($gridX.Count-1); $c++) {
    $x = [int](($gridX[$c] + $gridX[$c+1]) / 2.0)
    $line = ''
    for ($r = 0; $r -lt ($gridY.Count-1); $r++) {
        $y = [int](($gridY[$r] + $gridY[$r+1]) / 2.0)
        $ch = Classify $bmp.GetPixel($x, $y)
        $line += $ch
    }
    Write-Host "between col$c and $($c+1) at x=$x : $line"
}

# Sample ON the grid lines themselves (thin lines)
Write-Host "`n--- ON H grid lines ---"
for ($ri = 1; $ri -lt ($gridY.Count-1); $ri++) {
    $y = $gridY[$ri]
    $line = ''
    for ($c = 0; $c -lt ($gridX.Count-1); $c++) {
        $x = [int](($gridX[$c] + $gridX[$c+1]) / 2.0)
        $line += (Classify $bmp.GetPixel($x, $y))
    }
    Write-Host "hline ri=$ri y=$y : $line"
}

Write-Host "`n--- ON V grid lines ---"
for ($ci = 1; $ci -lt ($gridX.Count-1); $ci++) {
    $x = $gridX[$ci]
    $line = ''
    for ($r = 0; $r -lt ($gridY.Count-1); $r++) {
        $y = [int](($gridY[$r] + $gridY[$r+1]) / 2.0)
        $line += (Classify $bmp.GetPixel($x, $y))
    }
    Write-Host "vline ci=$ci x=$x : $line"
}

$bmp.Dispose()
