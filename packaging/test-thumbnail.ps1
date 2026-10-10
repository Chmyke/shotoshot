# Proves that Explorer gets a thumbnail for a ".shoto" file on this user account, the way the
# desktop and Explorer ask for one (IShellItemImageFactory, thumbnail only: an icon fallback
# counts as a failure). Writes a fresh ".shoto" file under a new name, so the thumbnail cache
# cannot answer for it, and checks that the bitmap has the image's own size.
# Needs the ".shoto" association of an installed Shotoshot (packaging/shotoshot.iss): run it
# after an installation, from the real user session (test-installer.ps1 does).
# Exit code 0 = PASS, 1 = FAIL. -Source: a PNG or .shoto to use; by default the script paints
# its own 240x140 PNG, a size no icon has. -OutPng: saves the thumbnail for a human look.
param(
    [string]$Source = "",
    [string]$OutPng = ""
)
$ErrorActionPreference = 'Stop'

$shell = @"
using System;
using System.Runtime.InteropServices;
public static class ShotoThumbnailProbe {
    [ComImport, Guid("bcc18b79-ba16-442f-80c4-8a59c30c463b"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface IShellItemImageFactory {
        [PreserveSig] int GetImage(SIZE size, int flags, out IntPtr phbm);
    }
    [StructLayout(LayoutKind.Sequential)] public struct SIZE { public int cx; public int cy; }
    [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
    public static extern void SHCreateItemFromParsingName(string path, IntPtr pbc, ref Guid riid,
        [MarshalAs(UnmanagedType.Interface)] out IShellItemImageFactory ppv);
    [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr h);
    public const int SIIGBF_THUMBNAILONLY = 0x8;
    // Returns the HRESULT; on success w and h hold the bitmap size and outPng (if set) its PNG.
    public static int Probe(string path, string outPng, out int w, out int h) {
        Guid iid = typeof(IShellItemImageFactory).GUID;
        IShellItemImageFactory factory;
        SHCreateItemFromParsingName(path, IntPtr.Zero, ref iid, out factory);
        SIZE size; size.cx = 256; size.cy = 256;
        IntPtr hbm;
        int hr = factory.GetImage(size, SIIGBF_THUMBNAILONLY, out hbm);
        w = 0; h = 0;
        if (hr == 0 && hbm != IntPtr.Zero) {
            using (System.Drawing.Bitmap b = System.Drawing.Image.FromHbitmap(hbm)) {
                w = b.Width; h = b.Height;
                if (outPng.Length > 0) { b.Save(outPng, System.Drawing.Imaging.ImageFormat.Png); }
            }
            DeleteObject(hbm);
        }
        return hr;
    }
}
"@
Add-Type -TypeDefinition $shell -ReferencedAssemblies System.Drawing

$dir = Join-Path $env:TEMP 'shotoshot-thumbnail-probe'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$stamp = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
$sample = Join-Path $dir "sample-$stamp.shoto"

if ($Source) {
    $image = [System.Drawing.Image]::FromFile((Resolve-Path -LiteralPath $Source).Path)
    $expectedSize = "$($image.Width)x$($image.Height)"
    $image.Dispose()
    Copy-Item -LiteralPath $Source -Destination $sample
} else {
    # A plain gradient with a frame: any decodable PNG does, the provider only decodes.
    $expectedSize = '240x140'
    $bitmap = New-Object System.Drawing.Bitmap 240, 140
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $from = New-Object System.Drawing.Point -ArgumentList 0, 0
    $to = New-Object System.Drawing.Point -ArgumentList 240, 140
    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush -ArgumentList $from, $to, ([System.Drawing.Color]::SteelBlue), ([System.Drawing.Color]::Orange)
    $graphics.FillRectangle($brush, 0, 0, 240, 140)
    $graphics.DrawRectangle([System.Drawing.Pens]::White, 20, 20, 80, 60)
    $graphics.Dispose(); $brush.Dispose()
    $bitmap.Save($sample, [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
}

try {
    $w = 0; $h = 0
    $hr = [ShotoThumbnailProbe]::Probe($sample, $OutPng, [ref]$w, [ref]$h)
    $got = "{0}x{1}" -f $w, $h
    if ($hr -eq 0 -and $got -eq $expectedSize) {
        Write-Output "PASS  .shoto thumbnail is the image itself ($got, hr=0x00000000)"
        exit 0
    }
    Write-Output ("FAIL  .shoto thumbnail: hr=0x{0:X8}, size {1}, expected {2}" -f $hr, $got, $expectedSize)
    exit 1
} finally {
    Remove-Item -LiteralPath $sample -ErrorAction SilentlyContinue
}
