# Rebuild the D2 vector geometry at Windows icon sizes. Uses only Windows GDI+.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$sizes = @(16,20,24,32,48,256)
$frames = @()
foreach ($size in $sizes) {
    $large = New-Object System.Drawing.Bitmap 1024,1024
    $g = [System.Drawing.Graphics]::FromImage($large)
    $g.SmoothingMode = 'AntiAlias'
    $g.ScaleTransform(4,4)
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.AddArc(8,8,120,120,180,90)
    $path.AddArc(128,8,120,120,270,90)
    $path.AddArc(128,128,120,120,0,90)
    $path.AddArc(8,128,120,120,90,90)
    $path.CloseFigure()
    $blue = New-Object System.Drawing.Drawing2D.LinearGradientBrush ([System.Drawing.Point]::new(8,8)),([System.Drawing.Point]::new(248,248)),([System.Drawing.Color]::FromArgb(0,165,249)),([System.Drawing.Color]::FromArgb(0,125,245))
    $g.FillPath($blue,$path)
    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White),18
    $pen.StartCap = $pen.EndCap = 'Round'
    $corners = New-Object System.Drawing.Drawing2D.GraphicsPath
    $corners.AddLine(51,105,51,73)
    $corners.AddBezier(51,73,51,61,57,55,69,55)
    $corners.AddLine(69,55,103,55)
    $corners.StartFigure()
    $corners.AddLine(205,151,205,183)
    $corners.AddBezier(205,183,205,195,199,201,187,201)
    $corners.AddLine(187,201,153,201)
    $g.DrawPath($pen,$corners)
    $g.DrawEllipse($pen,85,85,86,86)
    $core = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(32,55,71))
    $g.FillEllipse($core,111,111,34,34)
    $small = New-Object System.Drawing.Bitmap $size,$size
    $resample = [System.Drawing.Graphics]::FromImage($small)
    $resample.InterpolationMode = 'HighQualityBicubic'
    $resample.PixelOffsetMode = 'HighQuality'
    $resample.DrawImage($large,0,0,$size,$size)
    $stream = New-Object System.IO.MemoryStream
    $small.Save($stream,[System.Drawing.Imaging.ImageFormat]::Png)
    $frames += ,$stream.ToArray()
    $small.Save((Join-Path $PSScriptRoot "shotoshot-$size.png"),[System.Drawing.Imaging.ImageFormat]::Png)
    $stream.Dispose(); $resample.Dispose(); $small.Dispose(); $core.Dispose()
    $corners.Dispose(); $pen.Dispose(); $blue.Dispose(); $path.Dispose(); $g.Dispose(); $large.Dispose()
}
$file = [System.IO.File]::Create((Join-Path $PSScriptRoot 'shotoshot.ico'))
$writer = New-Object System.IO.BinaryWriter $file
$writer.Write([UInt16]0); $writer.Write([UInt16]1); $writer.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($index = 0; $index -lt $sizes.Count; $index++) {
    $dimension = $sizes[$index] % 256
    $writer.Write([byte]$dimension); $writer.Write([byte]$dimension)
    $writer.Write([UInt16]0); $writer.Write([UInt16]1); $writer.Write([UInt16]32)
    $writer.Write([UInt32]$frames[$index].Length); $writer.Write([UInt32]$offset)
    $offset += $frames[$index].Length
}
foreach ($frame in $frames) { $writer.Write([byte[]]$frame) }
$writer.Dispose()
