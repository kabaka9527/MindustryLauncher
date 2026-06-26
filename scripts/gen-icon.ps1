param(
    [string]$SourcePng = "Assets\Square150x150Logo.scale-200.png",
    [string]$OutputIco = "Assets\AppIcon.ico"
)

Add-Type -AssemblyName System.Drawing

$sourcePath = Join-Path $PSScriptRoot "..\$SourcePng"
$outputPath  = Join-Path $PSScriptRoot "..\$OutputIco"

$src = [System.Drawing.Image]::FromFile((Resolve-Path $sourcePath))

$sizes = @(16, 24, 32, 48, 64, 96, 128, 256)

$icoStream = [System.IO.MemoryStream]::new()
$writer = [System.IO.BinaryWriter]::new($icoStream)

# ICO header
$writer.Write([UInt16]0)      # reserved
$writer.Write([UInt16]1)      # ICO type
$writer.Write([UInt16]$sizes.Count)  # image count

$offset = 6 + $sizes.Count * 16
$imageStreams = @()

Write-Host "Generating icon from $([System.IO.Path]::GetFileName($sourcePath)) ($($src.Width)x$($src.Height))"

foreach ($size in $sizes) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.DrawImage($src, 0, 0, $size, $size)
    $g.Dispose()

    $ms = [System.IO.MemoryStream]::new()
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $data = $ms.ToArray()
    $ms.Dispose()
    $bmp.Dispose()

    $imageStreams += $data

    Write-Host "  $($size)x$($size) -> $($data.Length) bytes (PNG)"
}

for ($i = 0; $i -lt $sizes.Count; $i++) {
    $w = $sizes[$i]
    $h = $sizes[$i]
    $bpp = 32
    $writer.Write([Byte]($w -eq 256 ? 0 : $w))  # width (0 = 256)
    $writer.Write([Byte]($h -eq 256 ? 0 : $h))  # height (0 = 256)
    $writer.Write([Byte]0)   # color palette
    $writer.Write([Byte]0)   # reserved
    $writer.Write([UInt16]1) # color planes
    $writer.Write([UInt16]$bpp)  # bits per pixel
    $writer.Write([UInt32]$imageStreams[$i].Length)
    $writer.Write([UInt32]$offset)
    $offset += $imageStreams[$i].Length
}

foreach ($data in $imageStreams) {
    $writer.Write($data)
}

$writer.Flush()
[System.IO.File]::WriteAllBytes($outputPath, $icoStream.ToArray())

$writer.Dispose()
$icoStream.Dispose()
$src.Dispose()

$finalSize = [math]::Round((Get-Item $outputPath).Length / 1KB, 1)
Write-Host "Written to $OutputIco ($finalSize KB)"
