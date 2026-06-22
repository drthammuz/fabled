Add-Type -AssemblyName System.Drawing
$bmp = [System.Drawing.Bitmap]::FromFile("$PSScriptRoot\..\userinput\examplegrid.png")

function GetEdgeColor($x0,$y0,$x1,$y1) {
    $counts = @{ B=0; G=0; R=0; Y=0; K=0 }
    $steps = [Math]::Max([Math]::Abs($x1-$x0), [Math]::Abs($y1-$y0))
    if ($steps -lt 1) { $steps = 1 }
    for ($i=0; $i -le $steps; $i++) {
        $t = $i/$steps
        $x = [int]($x0 + ($x1-$x0)*$t)
        $y = [int]($y0 + ($y1-$y0)*$t)
        $c = $bmp.GetPixel($x,$y)
        if ($c.B -gt 180 -and $c.R -lt 120 -and $c.G -lt 120) { $counts.B++ }
        elseif ($c.G -gt 150 -and $c.R -lt 120 -and $c.B -lt 120) { $counts.G++ }
        elseif ($c.R -gt 150 -and $c.G -lt 120 -and $c.B -lt 120) { $counts.R++ }
        elseif ($c.R -gt 150 -and $c.G -gt 150 -and $c.B -lt 120) { $counts.Y++ }
        elseif ($c.R -lt 80 -and $c.G -lt 80 -and $c.B -lt 80) { $counts.K++ }
    }
    $best='.'; $bestN=0
    foreach ($k in @('R','G','Y','B','K')) {
        if ($counts[$k] -gt $bestN) { $bestN=$counts[$k]; $best=$k }
    }
    if ($bestN -lt 2) { return '.' }
    return $best
}

$gridX = @(5, 48, 91, 134, 177, 220, 263, 306, 349, 390)
$gridY = @(6, 52, 98, 145, 191, 237, 284)

Write-Host "H edges (gi=1..6 between rows, col=cell):"
for ($gi = 1; $gi -le 6; $gi++) {
    $y = $gridY[$gi]
    $line = ""
    for ($ci = 0; $ci -lt 9; $ci++) {
        $x0 = $gridX[$ci]+3; $x1 = $gridX[$ci+1]-3
        $line += (GetEdgeColor $x0 $y $x1 $y)
    }
    Write-Host "  gi=$gi row$($gi-1)|$gi : $line"
}

Write-Host "V edges (xi=1..9 between cols, row=cell):"
for ($xi = 1; $xi -le 9; $xi++) {
    $x = $gridX[$xi]
    $line = ""
    for ($ri = 0; $ri -lt 6; $ri++) {
        $y0 = $gridY[$ri]+3; $y1 = $gridY[$ri+1]-3
        $line += (GetEdgeColor $x $y0 $x $y1)
    }
    Write-Host "  xi=$xi col$($xi-1)|$xi : $line"
}

Write-Host "Yellow cells:"
for ($ri=0; $ri -lt 6; $ri++) {
  for ($ci=0; $ci -lt 9; $ci++) {
    $x=[int](($gridX[$ci]+$gridX[$ci+1])/2)
    $y=[int](($gridY[$ri]+$gridY[$ri+1])/2)
    $c=$bmp.GetPixel($x,$y)
    if ($c.R -gt 150 -and $c.G -gt 150) { Write-Host "  Y at row$ri col$ci" }
  }
}

$bmp.Dispose()
