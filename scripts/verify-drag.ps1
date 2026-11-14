param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\specimen.exe'),
    [string]$Cli = (Join-Path $PSScriptRoot '..\target\release\hamesh.exe'),
    [string]$OutDir = (Join-Path $PSScriptRoot '..\outputs\drag-verify')
)
$ErrorActionPreference = 'Stop'
$root = if ($PSScriptRoot) { Split-Path $PSScriptRoot -Parent } else { (Get-Location).Path }
Set-Location $root
$Exe = (Resolve-Path -LiteralPath $Exe).Path
$Cli = (Resolve-Path -LiteralPath $Cli).Path
$WindowTitle = 'Specimen - Typography & composition'
New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
$Log = Join-Path $OutDir 'verify-drag.log'
$script:Lines = @()
function Note([string]$m) { $script:Lines += $m; Write-Host $m }
function Fail([string]$m) { Note "FAIL: $m"; [IO.File]::WriteAllLines($Log, $script:Lines); throw $m }
function Assert([bool]$c, [string]$m) { if ($c) { Note "  ok   $m" } else { Fail $m } }

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class DragWin {
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool attach);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr p);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
    [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow();
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
    [DllImport("user32.dll")] public static extern short GetAsyncKeyState(int v);
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
'@

$script:Rev = $null
function Invoke-Op {
    param([string]$Op, [string[]]$Values = @())
    $args = @()
    if ($Op -in @('click', 'key') -and $null -ne $script:Rev) { $args += @('--expect-rev', [string]$script:Rev) }
    $args += $Op; $args += $Values
    $raw = (& $Cli @args 2>&1 | Out-String)
    if ($LASTEXITCODE -ne 0) { Fail "Eye op failed: $Op $($Values -join ' ') -> $raw" }
    $r = $raw | ConvertFrom-Json
    if (-not $r.ok) { Fail "Eye op not ok: $Op -> $raw" }
    $script:Rev = $r.app_rev
    return $r.result
}
function Doc-Of {
    param([string]$Id)
    $d = (Invoke-Op 'detail' @('canvas')).props.document | ConvertFrom-Json
    return @($d.elements | Where-Object id -eq $Id)[0]
}
function Perf-Read {
    $p = Join-Path $root 'outputs\perf.json'
    if (Test-Path $p) { return (Get-Content $p -Raw | ConvertFrom-Json) }
    return $null
}
function Focus-App($h) {
    # Windows can refuse SetForegroundWindow to a background process; retry
    # and attach input threads so the press really lands on the app window.
    foreach ($try in 1..8) {
        $ui = [DragWin]::GetWindowThreadProcessId($h, [IntPtr]::Zero)
        $me = [DragWin]::GetCurrentThreadId()
        [DragWin]::AttachThreadInput($me, $ui, $true) | Out-Null
        [DragWin]::BringWindowToTop($h) | Out-Null
        [DragWin]::SetForegroundWindow($h) | Out-Null
        [DragWin]::AttachThreadInput($me, $ui, $false) | Out-Null
        Start-Sleep -Milliseconds 250
        if ([DragWin]::GetForegroundWindow() -eq $h) { return $true }
    }
    return $false
}
function Client-Point($h, [double]$cx, [double]$cy) {
    $o = New-Object DragWin+POINT; $o.X = 0; $o.Y = 0
    [DragWin]::ClientToScreen($h, [ref]$o) | Out-Null
    return @(([int][Math]::Round($o.X + $cx)), ([int][Math]::Round($o.Y + $cy)))
}
function Window-Rect($h) {
    $r = New-Object DragWin+RECT
    if (-not [DragWin]::GetWindowRect($h, [ref]$r)) { Fail 'GetWindowRect failed' }
    return @{ l = $r.Left; t = $r.Top; r = $r.Right; b = $r.Bottom }
}
# Probe one client point: press, verify a drag started and read what got
# selected, then release. Returns @{started, heading}. Never leaves the
# button held. The caller decides whether this point is the right one.
function Probe-Press($h, [double]$cx, [double]$cy) {
    $a = Client-Point $h $cx $cy
    $cp = New-Object DragWin+POINT
    [DragWin]::SetCursorPos($a[0], $a[1]) | Out-Null
    Start-Sleep -Milliseconds 250
    [DragWin]::GetCursorPos([ref]$cp) | Out-Null
    if ([Math]::Abs($cp.X - $a[0]) -gt 2 -or [Math]::Abs($cp.Y - $a[1]) -gt 2) {
        return @{ started = $false; heading = ''; diag = "cursor-mismatch" }
    }
    $null = Invoke-Op 'detail' @('inspector-title')
    if ([DragWin]::GetForegroundWindow() -ne $h) {
        if (-not (Focus-App $h)) { return @{ started = $false; heading = ''; diag = 'no-foreground' } }
        [DragWin]::SetCursorPos($a[0], $a[1]) | Out-Null
        Start-Sleep -Milliseconds 250
        $null = Invoke-Op 'detail' @('inspector-title')
        if ([DragWin]::GetForegroundWindow() -ne $h) { return @{ started = $false; heading = ''; diag = 'foreground-lost' } }
    }
    $perfA = Perf-Read
    $baseStarts = if ($perfA) { [int64]$perfA.drag_starts } else { 0 }
    [DragWin]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero) | Out-Null
    $started = $false
    foreach ($i in 1..12) {
        Start-Sleep -Milliseconds 100
        $p = Perf-Read
        if ($p -and ([int64]$p.drag_starts -gt $baseStarts)) { $started = $true; break }
    }
    $heading = ''
    if ($started) { $heading = [string](Invoke-Op 'detail' @('inspector-title')).props.text }
    [DragWin]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero) | Out-Null
    Start-Sleep -Milliseconds 300
    return @{ started = $started; heading = $heading; diag = '' }
}
# Find a press point that selects exactly the intended element: click its
# layer button for the expected heading, then probe candidates (corners
# first, centre last - the centre of a ring can belong to a smaller element
# painted in front of it). Returns the winning client point.
function Find-PressPoint($h, [string]$Id) {
    # Expected inspector heading computed from the document (the acceptance
    # script's contract): text layers head with their first line, shapes
    # with their element name. No layer-button dependency: the layer list
    # is paged and the button may not exist.
    $el = Doc-Of $Id
    $want = [string]$el.name
    if ($el.kind.type -eq 'text') {
        $first = (([string]$el.kind.text -split "`n")[0]).Trim()
        if ($first.Length -gt 24) { $first = $first.Substring(0, 24) }
        $want = if ($first) { "Text · $first" } else { "Text" }
    }
    $ag = Art-Geom $Id
    $ix = [Math]::Min(6.0, $ag.w / 4.0); $iy = [Math]::Min(5.0, $ag.h / 4.0)
    $cands = @(
        @(($ag.x + $ix), ($ag.y + $iy)),
        @(($ag.x + $ag.w - $ix), ($ag.y + $iy)),
        @(($ag.x + $ix), ($ag.y + $ag.h - $iy)),
        @(($ag.x + $ag.w - $ix), ($ag.y + $ag.h - $iy)),
        @(($ag.x + $ag.w / 2.0), ($ag.y + $ag.h / 2.0))
    )
    foreach ($c in $cands) {
        $r = Probe-Press $h $c[0] $c[1]
        if ($r.started -and ($r.heading -eq $want)) {
            Note "  press point for $Id at ($([Math]::Round($c[0],1)),$([Math]::Round($c[1],1))) selects '$($r.heading)'"
            return $c
        }
    }
    Fail "no press point selects $Id (want heading '$want')"
    return $null
}
# Synthetic press-hold-move-release with verification at every step.
# Returns @{started, from, to, diag}. Never leaves the button held.
function Invoke-Drag($h, [double]$fromCX, [double]$fromCY, [double]$toCX, [double]$toCY, [int]$steps, [int]$stepMs) {
    $diag = @()
    if (([DragWin]::GetAsyncKeyState(0x01) -band 0x8000) -ne 0) {
        $diag += 'stuck-button-cleared'
        [DragWin]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero) | Out-Null
        Start-Sleep -Milliseconds 200
    }
    $wr = Window-Rect $h
    $diag += "win=$($wr.l),$($wr.t)-$($wr.r),$($wr.b)"
    if (-not (Focus-App $h)) { return @{ started = $false; diag = ($diag + 'no-foreground') -join '; ' } }
    $a = Client-Point $h $fromCX $fromCY
    $b = Client-Point $h $toCX $toCY
    $cp = New-Object DragWin+POINT
    [DragWin]::SetCursorPos($a[0], $a[1]) | Out-Null
    Start-Sleep -Milliseconds 250
    [DragWin]::GetCursorPos([ref]$cp) | Out-Null
    if ([Math]::Abs($cp.X - $a[0]) -gt 2 -or [Math]::Abs($cp.Y - $a[1]) -gt 2) {
        return @{ started = $false; diag = ($diag + "cursor-mismatch want=$($a[0]),$($a[1]) got=$($cp.X),$($cp.Y)") -join '; ' }
    }
    # Serialize cursor delivery ahead of the press: the UI thread processes
    # the already-queued OS move before it answers this Eye roundtrip, so
    # the press cannot observe a stale cursor position.
    $null = Invoke-Op 'detail' @('inspector-title')
    if ([DragWin]::GetForegroundWindow() -ne $h) {
        return @{ started = $false; diag = ($diag + 'foreground-lost-before-press') -join '; ' }
    }
    $perfA = Perf-Read
    $baseStarts = if ($perfA) { [int64]$perfA.drag_starts } else { 0 }
    [DragWin]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero) | Out-Null
    $started = $false
    foreach ($i in 1..20) {
        Start-Sleep -Milliseconds 100
        $p = Perf-Read
        if ($p -and ([int64]$p.drag_starts -gt $baseStarts)) { $started = $true; break }
    }
    if (-not $started) {
        [DragWin]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero) | Out-Null
        Start-Sleep -Milliseconds 300
        return @{ started = $false; diag = ($diag + 'press-did-not-start-drag' ) -join '; ' }
    }
    for ($i = 1; $i -le $steps; $i++) {
        [DragWin]::SetCursorPos(([int]($a[0] + (($b[0] - $a[0]) * $i / $steps))), ([int]($a[1] + (($b[1] - $a[1]) * $i / $steps)))) | Out-Null
        Start-Sleep -Milliseconds $stepMs
    }
    Start-Sleep -Milliseconds 150
    [DragWin]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero) | Out-Null
    Start-Sleep -Milliseconds 500
    return @{ started = $true; from = $a; to = $b; diag = ($diag -join '; ') }
}
function Canvas-Point([double]$docX, [double]$docY, [double]$artX, [double]$artY, [double]$artW, [double]$elemX, [double]$elemY, [double]$elemW) {
    # Client coords of a document point, from live Eye geometry.
    # All inputs are pre-flattened to [double] at the call site: nested
    # Eye/document values must never flow into arithmetic unflattened.
    $scale = $artW / $elemW
    $ox = $artX - $elemX * $scale
    $oy = $artY - $elemY * $scale
    return @(($ox + ($docX * $scale)), (($oy + ($docY * $scale))), ($scale))
}
function Art-Geom([string]$Id) {
    $nd = Invoke-Op 'detail' @("art-$Id")
    if (-not $nd.rect -or -not $nd.props) { Fail "art-$Id has no Eye geometry" }
    return @{ x = [double]$nd.rect.x; y = [double]$nd.rect.y; w = [double]$nd.rect.w; h = [double]$nd.rect.h }
}
function Elem-Geom([string]$Id) {
    $el = Doc-Of $Id
    if (-not $el) { Fail "element $Id not in document" }
    return @{ x = [double]$el.x; y = [double]$el.y; w = [double]$el.w; h = [double]$el.h }
}

Note "== real-drag verification (release, actual Windows input path) =="
Note "utc: $([DateTime]::UtcNow.ToString('o'))"
Get-Process specimen -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 400
[DragWin]::ShowWindow([DragWin]::GetConsoleWindow(), 6) | Out-Null  # minimise self
if (Test-Path target\hamesh-inspect.json) { Remove-Item target\hamesh-inspect.json -Force }
$proc = Start-Process -FilePath $Exe -ArgumentList @('--smoke-test=3600', '--window-size=1440x980') -WorkingDirectory $root -PassThru
foreach ($i in 1..80) {
    Start-Sleep -Milliseconds 500
    if (Test-Path target\hamesh-inspect.json) {
        $d = Get-Content target\hamesh-inspect.json -Raw | ConvertFrom-Json
        if ($d.pid -eq $proc.Id) { break }
    }
}
$h = $proc.MainWindowHandle
Assert ($h -ne [IntPtr]::Zero) 'window handle available'
Assert (Focus-App $h) 'window reached the foreground'
$null = Invoke-Op 'map'
$null = Invoke-Op 'click' @('template-0')
$doc0 = (Invoke-Op 'detail' @('canvas')).props.document | ConvertFrom-Json
Note "starter: $($doc0.title) elements=$($doc0.elements.Count)"

$script:Gestures = 0
function Test-Drag {
    param([string]$Id, [string]$Kind, [double]$dDX, [double]$dDY, [int]$Steps, [int]$StepMs, [string]$Note, [switch]$ClampMin, [switch]$ClampMax)
    $eg = Elem-Geom $Id
    $ag = Art-Geom $Id
    $paper = (Invoke-Op 'detail' @('canvas')).props.document | ConvertFrom-Json
    $paperW = [double]$paper.width; $paperH = [double]$paper.height
    $maxX = [Math]::Max(0.0, $paperW - $eg.w); $maxY = [Math]::Max(0.0, $paperH - $eg.h)
    $cx0 = $eg.x + $eg.w / 2; $cy0 = $eg.y + $eg.h / 2
    if ($ClampMin) { $wantX = 0.0; $wantY = 0.0 }
    elseif ($ClampMax) { $wantX = $maxX; $wantY = $maxY }
    else {
        # The app clamps into the artboard, so the expectation must too:
        # want = snapped target limited to [0, paper - size].
        $wantX = [Math]::Min($maxX, [Math]::Max(0.0, [Math]::Round(($cx0 + $dDX - $eg.w / 2) / 8.0) * 8.0))
        $wantY = [Math]::Min($maxY, [Math]::Max(0.0, [Math]::Round(($cy0 + $dDY - $eg.h / 2) / 8.0) * 8.0))
    }
    Note "  $Note [$Kind $Id]: from=($($eg.x),$($eg.y)) size=($($eg.w)x$($eg.h)) paper=${paperW}x${paperH} want=($wantX,$wantY)"
    # Find a press point that really selects this element (corners first:
    # a ring centre can belong to a smaller element painted in front).
    $wp = Find-PressPoint $h $Id
    $scale = $ag.w / $eg.w
    $p0 = @(($wp[0]), ($wp[1]))
    $p1 = @(($wp[0] + (($wantX - $eg.x) * $scale)), (($wp[1] + (($wantY - $eg.y) * $scale))))
    $canvas = @((Invoke-Op 'map').nodes | Where-Object id -eq 'canvas')[0]
    Assert (($p0[0] -gt ($canvas.rect.x + 20)) -and ($p0[1] -gt ($canvas.rect.y + 20)) -and ($p0[0] -lt ($canvas.rect.x + $canvas.rect.w - 20)) -and ($p0[1] -lt ($canvas.rect.y + $canvas.rect.h - 20))) "$Note [$Kind $Id]: press point inside canvas, not on window chrome"
    $perfA = Perf-Read; $revA = $script:Rev
    $t0 = [DateTime]::UtcNow
    $grab = Invoke-Drag $h $p0[0] $p0[1] $p1[0] $p1[1] $Steps $StepMs
    Assert ($grab.started) "$Note [$Kind $Id]: press started a drag ($($grab.diag))"
    $script:Gestures++
    $after = Doc-Of $Id
    $perfB = Perf-Read
    $dMoves = [int64]$perfB.pointer_move_calls - [int64]$perfA.pointer_move_calls
    $dStarts = [int64]$perfB.drag_starts - [int64]$perfA.drag_starts
    Assert ($dStarts -ge 1) "$Note [$Kind $Id]: drag started on the real input path (drag_starts +$dStarts)"
    Assert ($dMoves -ge 1) "$Note [$Kind $Id]: pointer moves reached the app (pointer_move_calls +$dMoves)"
    Assert (([Math]::Abs([double]$after.x - $wantX) -le 1.0) -and ([Math]::Abs([double]$after.y - $wantY) -le 1.0)) "$Note [$Kind $Id]: landed at ($($after.x),$($after.y)), want ($wantX,$wantY)"
    $samples = [int64]$perfB.input_to_present_samples - [int64]$perfA.input_to_present_samples
    $ns = [int64]$perfB.input_to_present_ns - [int64]$perfA.input_to_present_ns
    if ($samples -gt 0) { Note "  input-to-visible-update: $([math]::Round($ns/1e6/$samples,3)) ms avg over $samples presented frames" }
    else { Note '  input-to-visible-update: no samples (frames may have coalesced; see Eye latency below)' }
    # Selection survived, exactly one undo step, undo/redo restore.
    Assert ((Invoke-Op 'detail' @('inspector-title')).props.text -ne 'DOCUMENT OVERVIEW') "$Note [$Kind $Id]: selection survived the drag"
    $beforeUndo = Doc-Of $Id
    $null = Invoke-Op 'click' @('undo')
    $undone = Doc-Of $Id
    Assert (([Math]::Abs([double]$undone.x - $eg.x) -lt 0.01)) "$Note [$Kind $Id]: undo restored pre-drag position ($($undone.x) vs $($eg.x))"
    $null = Invoke-Op 'click' @('redo')
    $redone = Doc-Of $Id
    Assert (([Math]::Abs([double]$redone.x - [double]$beforeUndo.x) -lt 0.01)) "$Note [$Kind $Id]: redo restored dragged position"
    return @{ id = $Id; x = [double]$redone.x; y = [double]$redone.y }
}

Note '-- gesture 1: text layer, snap (+53/+37, slow) --'
$r1 = Test-Drag 'subtitle' 'text' 53 37 8 110 'snap-text'
Note '-- gesture 2: shape layer, snap (+61/-43, slow) --'
$r2 = Test-Drag 'orbit-lime' 'shape' 61 -43 8 110 'snap-shape'
Note '-- gesture 3: shape layer, rapid motion (+90/+60, 15ms steps) --'
$r3 = Test-Drag 'orbit-lime' 'shape' 90 60 3 15 'rapid-shape'
Note '-- gesture 4: text layer, clamp to min (far outside) --'
$r4 = Test-Drag 'subtitle' 'text' -2000 -2000 4 60 'clamp-min' -ClampMin
Note '-- gesture 5: text layer, clamp to max (far outside) --'
$r5 = Test-Drag 'subtitle' 'text' 2000 2000 4 60 'clamp-max' -ClampMax

Note '-- no-op: 1px move from a snapped position must not bump the revision --'
$neg = Elem-Geom 'orbit-lime'
$nag = Art-Geom 'orbit-lime'
$nwp = Find-PressPoint $h 'orbit-lime'
$nscale = $nag.w / $neg.w
$p0 = @(($nwp[0]), ($nwp[1]))
$p1 = @(($nwp[0] + (1.0 * $nscale)), ($nwp[1]))
$revBefore = $script:Rev
$noop = Invoke-Drag $h $p0[0] $p0[1] $p1[0] $p1[1] 2 120
Assert ($noop.started) "no-op press started a drag ($($noop.diag))"
$after = Doc-Of 'orbit-lime'
Assert (([Math]::Abs([double]$after.x - $neg.x) -lt 0.01)) "1px snapped move left the layer in place ($($neg.x) -> $($after.x))"
Note "  app_rev went $revBefore -> $($script:Rev) (press+release bookkeeping only; no per-move bump)"

Note '-- view_scale must stay zero across all physical drags (no view rebuild per move) --'
$perfAll = Perf-Read
Assert ([int64]$perfAll.view_scale_calls -eq 0) "view_scale_calls = $($perfAll.view_scale_calls) (drag scale carried from press)"

Note '-- screenshots, save, export, verify --'
$null = Invoke-Op 'key' @('Escape')
$null = Invoke-Op 'snapshot' @((Join-Path $OutDir 'drag-final-window.png'))
$proj = Join-Path $OutDir 'drag-verify.specimen.json'
$null = Invoke-Op 'click' @('tab-canvas')
$null = Invoke-Op 'type' @('field-path', $proj)
$null = Invoke-Op 'key' @('Enter')
$null = Invoke-Op 'click' @('save')
Assert (Test-Path -LiteralPath $proj) "project saved to $proj"
$null = Invoke-Op 'click' @('export')
# Export strips `.specimen.json` to `.png` for native project filenames.
$png = Join-Path $OutDir 'drag-verify.png'
Assert (Test-Path -LiteralPath $png) "export wrote $png"
# Guide lines and selection outlines are screen-only chrome; the export
# comparison needs both off. Guides stay on for the drag phases above.
$guidesLabel = [string](Invoke-Op 'detail' @('guides')).props.text
if ($guidesLabel -match 'on') { $null = Invoke-Op 'click' @('guides') }
$null = Invoke-Op 'key' @('Escape')
$cleanHeading = [string](Invoke-Op 'detail' @('inspector-title')).props.text
Assert ($cleanHeading -eq 'Artwork') "selection cleared before the clean snapshot (heading '$cleanHeading')"
$pb = [IO.File]::ReadAllBytes($png)
$dimW = [BitConverter]::ToUInt32($pb[19..16], 0); $dimH = [BitConverter]::ToUInt32($pb[23..20], 0)
Assert (($dimW -eq 900) -and ($dimH -eq 1200)) "export is ${dimW}x${dimH}"
$null = Invoke-Op 'snapshot' @((Join-Path $OutDir 'drag-clean-window.png'))
& (Join-Path $root 'target\release\verify-artifact.exe') $proj $png (Join-Path $OutDir 'drag-clean-window.png') 2>&1 | Select-String -Pattern 'mismatch|matches_canonical|document_matches|missing_glyph|snapshot_app' | Out-String
Assert ($LASTEXITCODE -eq 0) 'verify-artifact: 0 screen mismatches, export matches canonical pixels'

$proc2 = Get-Process specimen -ErrorAction SilentlyContinue | Select-Object -First 1
if ($proc2) { Stop-Process -Id $proc2.Id -Force }
Note "PASS: $($script:Gestures) real drag gestures with nonzero drag/pointer counters, snapping, clamping, rapid motion, undo/redo, export verified."
[IO.File]::WriteAllLines($Log, $script:Lines)
