param(
    [string]$Label = 'run',
    [string]$Cli = '',
    [string]$Exe = '',
    [string]$OutFile = ''
)
$ErrorActionPreference = 'Stop'
$root = if ($PSScriptRoot) { Split-Path $PSScriptRoot -Parent } else { (Get-Location).Path }
Set-Location $root
if (-not $Cli) { $Cli = (Join-Path $root 'target\debug\hamesh.exe') }
if (-not $Exe) { $Exe = (Join-Path $root 'target\debug\specimen.exe') }
if (-not $OutFile) { $OutFile = (Join-Path $root ("perf-report-{0}.json" -f $Label)) }
$Evidence = Join-Path $root 'outputs'
New-Item -ItemType Directory -Path $Evidence -Force | Out-Null
$WindowTitle = 'Specimen - Typography & composition'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class Win32Perf {
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr p);
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool attach);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
}
'@

$script:Rev = $null
$script:Ops = @()

function Invoke-Op {
    param([string]$Op, [string[]]$Values = @())
    $args = @()
    if ($Op -in @('click', 'key') -and $null -ne $script:Rev) { $args += @('--expect-rev', [string]$script:Rev) }
    $args += $Op
    $args += $Values
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $raw = (& $Cli @args 2>&1 | Out-String)
    $sw.Stop()
    $ms = $sw.Elapsed.TotalMilliseconds
    $ok = $false
    try { $r = $raw | ConvertFrom-Json; $ok = [bool]$r.ok } catch { $ok = $false }
    if ($ok) { $script:Rev = $r.app_rev }
    $script:Ops += [pscustomobject]@{ op = $Op; ms = $ms; ok = $ok }
    return $(if ($ok) { $r.result } else { $null })
}
function Close-App {
    $p = Get-Process specimen -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $p) { return }
    if ($p.MainWindowHandle -ne [IntPtr]::Zero) { [Win32Perf]::ShowWindow($p.MainWindowHandle, 0) | Out-Null }
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    $p.WaitForExit(8000) | Out-Null
}
function Start-App([int]$TimeoutSec = 60) {
    if (Test-Path target\hamesh-inspect.json) { Remove-Item target\hamesh-inspect.json -Force }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $proc = Start-Process -FilePath $Exe -ArgumentList @('--smoke-test=3600', '--window-size=1440x980') -WorkingDirectory $root -PassThru
    $deadline = $sw.Elapsed.TotalSeconds + $TimeoutSec
    while ($sw.Elapsed.TotalSeconds -lt $deadline) {
        Start-Sleep -Milliseconds 25
        if (Test-Path target\hamesh-inspect.json) {
            $d = Get-Content target\hamesh-inspect.json -Raw | ConvertFrom-Json
            if ($d.pid -eq $proc.Id) {
                $m = Invoke-Op 'map'
                if ($m) { return [pscustomobject]@{ proc = $proc; ms = $sw.Elapsed.TotalMilliseconds } }
            }
        }
    }
    throw 'App did not become ready in time'
}
function Perf-Snapshot {
    $p = Join-Path $root 'target\perf.json'
    if (-not (Test-Path $p)) { return $null }
    # perf.json lives in the evidence dir; fall back to either location
    return (Get-Content $p -Raw | ConvertFrom-Json)
}
function Perf-Read {
    foreach ($p in @((Join-Path $Evidence 'perf.json'), (Join-Path $root 'target\perf.json'))) {
        if (Test-Path $p) {
            try { return (Get-Content $p -Raw | ConvertFrom-Json) } catch { }
        }
    }
    return $null
}
function Delta($a, $b) {
    if ($null -eq $a -or $null -eq $b) { return $null }
    $d = [ordered]@{}
    foreach ($p in $b.PSObject.Properties) {
        if ($p.Name -eq 'utc_unix') { continue }
        $before = 0
        $ap = $a.PSObject.Properties[$p.Name]
        if ($ap) { $before = [int64]$ap.Value }
        $after = [int64]$p.Value
        $d[$p.Name] = $after - $before
    }
    return [pscustomobject]$d
}
function Ms($list) {
    $v = @($list | Sort-Object)
    if ($v.Count -eq 0) { return $null }
    $sum = ($v | Measure-Object -Sum).Sum
    return [pscustomobject]@{
        n     = $v.Count
        min   = [math]::Round($v[0], 2)
        p50   = [math]::Round($v[[int]($v.Count / 2)], 2)
        avg   = [math]::Round($sum / $v.Count, 2)
        max   = [math]::Round($v[$v.Count - 1], 2)
        total = [math]::Round($sum, 2)
    }
}

$results = [ordered]@{}
$results.label = $Label
$results.utc = [DateTime]::UtcNow.ToString('o')
$results.profile = if ($Exe -like '*release*') { 'release' } else { 'debug' }

# ---- cold start ----
Close-App
Start-Sleep -Milliseconds 300
$cold = Start-App
$h = $cold.proc.MainWindowHandle
$results.cold_start_ms = [math]::Round($cold.ms, 1)
$script:Rev = $null
$null = Invoke-Op 'map'

# ---- baseline snapshot ----
Start-Sleep -Milliseconds 400
$null = Invoke-Op 'snapshot' @((Join-Path $Evidence ("perf-seed-{0}.png" -f $Label)))
Start-Sleep -Milliseconds 300
$a = Perf-Read
$results.perf_after_start = $a

$tScenario = [Diagnostics.Stopwatch]::StartNew()

# ---- select layers ----
$map = Invoke-Op 'map'
$layerIds = @($map.nodes | Where-Object { $_.id -like 'layer-*' } | ForEach-Object { $_.id })
if ($layerIds.Count -eq 0) { $layerIds = @('layer-title') }
foreach ($i in 0..([Math]::Min(11, $layerIds.Count - 1))) { $null = Invoke-Op 'click' @($layerIds[$i % $layerIds.Count]) }
$results.select = Ms(@($script:Ops | Where-Object { $_.op -eq 'click' } | ForEach-Object { $_.ms }))
$script:Ops = @()

# ---- arrow nudges (drag-like per-frame document work) ----
$null = Invoke-Op 'click' @($layerIds[0])
$nudgeStart = $script:Ops.Count
foreach ($i in 1..20) { $null = Invoke-Op 'key' @($(if ($i % 2) { 'Right' } else { 'Left' })) }
$results.nudge = Ms(@($script:Ops | Where-Object { $_.op -eq 'key' } | ForEach-Object { $_.ms }))
$results.nudge_failures = @($script:Ops | Where-Object { -not $_.ok }).Count
$script:Ops = @()

# ---- typing into a field ----
$null = Invoke-Op 'click' @('tab-type')
$map2 = Invoke-Op 'map'
$hasText = @($map2.nodes | Where-Object { $_.id -eq 'field-text' }).Count -gt 0
if (-not $hasText) {
    $art = @($map2.nodes | Where-Object { $_.id -like 'art-*' } | Select-Object -First 1)
    if ($art.Count -gt 0) {
        $n = $art[0].rect
        $null = Invoke-Op 'click' @("$($n.x + 3),$($n.y + 2)")
        $null = Invoke-Op 'click' @('tab-type')
    }
}
$words = @('SIGNAL', 'PULSE', 'DRIFT', 'PHASE', 'NOISE', 'WAVE', 'TRACE', 'FIELD')
$typeMs = @()
foreach ($w in $words) {
    $t1 = [Diagnostics.Stopwatch]::StartNew()
    $null = Invoke-Op 'type' @('field-text', $w)
    $null = Invoke-Op 'key' @('Enter')
    $t1.Stop()
    $typeMs += $t1.Elapsed.TotalMilliseconds
}
$results.type_commit = Ms($typeMs)
$results.type_failures = @($script:Ops | Where-Object { -not $_.ok }).Count
$script:Ops = @()

# ---- typography edits ----
$typoMs = @()
foreach ($s in @(48, 64, 80, 96, 72, 56)) {
    $t1 = [Diagnostics.Stopwatch]::StartNew()
    $null = Invoke-Op 'type' @('field-size', [string]$s)
    $null = Invoke-Op 'key' @('Enter')
    $t1.Stop()
    $typoMs += $t1.Elapsed.TotalMilliseconds
}
$results.typography = Ms($typoMs)
$script:Ops = @()

# ---- undo / redo ----
foreach ($i in 1..10) { $null = Invoke-Op 'click' @('undo') }
foreach ($i in 1..10) { $null = Invoke-Op 'click' @('redo') }
$results.undo_redo = Ms(@($script:Ops | ForEach-Object { $_.ms }))
$results.undo_redo_failures = @($script:Ops | Where-Object { -not $_.ok }).Count
$script:Ops = @()

# ---- zoom and layer paging ----
foreach ($i in 1..8) { $null = Invoke-Op 'click' @('zoom') }
foreach ($i in 1..6) { $null = Invoke-Op 'click' @('layers-up') }
foreach ($i in 1..6) { $null = Invoke-Op 'click' @('layers-down') }
$results.zoom_layers = Ms(@($script:Ops | ForEach-Object { $_.ms }))
$script:Ops = @()

# ---- physical drag (best effort; records whether it took) ----
$drag = [ordered]@{ attempted = $true }
try {
    # Windows can refuse SetForegroundWindow to a background process; retry
    # and attach input threads so the press really lands on the app window.
    $fgOk = $false
    foreach ($try in 1..6) {
        $ui = [Win32Perf]::GetWindowThreadProcessId($h, [IntPtr]::Zero)
        $me = [Win32Perf]::GetCurrentThreadId()
        [Win32Perf]::AttachThreadInput($me, $ui, $true) | Out-Null
        [Win32Perf]::BringWindowToTop($h) | Out-Null
        [Win32Perf]::SetForegroundWindow($h) | Out-Null
        [Win32Perf]::AttachThreadInput($me, $ui, $false) | Out-Null
        Start-Sleep -Milliseconds 200
        if ([Win32Perf]::GetForegroundWindow() -eq $h) { $fgOk = $true; break }
    }
    $drag.fg_ok = $fgOk
    $drag.fg = [string][Win32Perf]::GetForegroundWindow()
    $drag.h = [string]$h
    Start-Sleep -Milliseconds 200
    $doc = ((Invoke-Op 'detail' @('canvas')).props.document | ConvertFrom-Json)
    $pick = $null
    foreach ($e in $doc.elements) {
        $a = Invoke-Op 'detail' @("art-$($e.id)")
        if ($a -and $a.effectively_visible -and -not $a.clipped_by_parent -and $a.rect.w -gt 60 -and $a.rect.h -gt 30) {
            $pick = [pscustomobject]@{ e = $e; a = $a }; break
        }
    }
    if (-not $pick) { throw 'no draggable artwork element' }
    $scale = [double]$pick.a.rect.w / [double]$pick.e.w
    $ox = [double]$pick.a.rect.x - [double]$pick.e.x * $scale
    $oy = [double]$pick.a.rect.y - [double]$pick.e.y * $scale
    $org = New-Object Win32Perf+POINT; $org.X = 0; $org.Y = 0
    [Win32Perf]::ClientToScreen($h, [ref]$org) | Out-Null
    $cx = $ox + ([double]$pick.e.x + [double]$pick.e.w / 2) * $scale
    $cy = $oy + ([double]$pick.e.y + [double]$pick.e.h / 2) * $scale
    $sx = [int][Math]::Round($org.X + $cx); $sy = [int][Math]::Round($org.Y + $cy)
    $tx = $sx + 44; $ty = $sy + 27
    $before = [pscustomobject]@{ x = [double]$pick.e.x; y = [double]$pick.e.y }
    $tDrag = [Diagnostics.Stopwatch]::StartNew()
    [Win32Perf]::SetCursorPos($sx, $sy) | Out-Null
    Start-Sleep -Milliseconds 250
    [Win32Perf]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 300
    for ($i = 1; $i -le 8; $i++) {
        [Win32Perf]::SetCursorPos([int]($sx + ($tx - $sx) * $i / 8), [int]($sy + ($ty - $sy) * $i / 8)) | Out-Null
        Start-Sleep -Milliseconds 90
    }
    Start-Sleep -Milliseconds 200
    [Win32Perf]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    $tDrag.Stop()
    Start-Sleep -Milliseconds 500
    $doc2 = ((Invoke-Op 'detail' @('canvas')).props.document | ConvertFrom-Json)
    $after = @($doc2.elements | Where-Object id -eq $pick.e.id)[0]
    $dx = [double]$after.x - $before.x; $dy = [double]$after.y - $before.y
    $drag.moved = ([math]::Abs($dx) -gt 0.5 -or [math]::Abs($dy) -gt 0.5)
    $drag.snap_ok = ([math]::Abs(([double]$after.x / 8) - [math]::Round([double]$after.x / 8)) -lt 0.001)
    $drag.ms = [math]::Round($tDrag.Elapsed.TotalMilliseconds, 1)
    $drag.dx = [math]::Round($dx, 1); $drag.dy = [math]::Round($dy, 1)
    $drag.id = $after.id
    if ($drag.moved) { $null = Invoke-Op 'click' @('undo') }
} catch {
    $drag.error = [string]$_.Exception.Message
}
$results.drag = [pscustomobject]$drag
$script:Ops = @()

# ---- window resize ----
$h = $cold.proc.MainWindowHandle
$resizeMs = @()
foreach ($wh in @(@(1600, 1020), @(1440, 980), @(1720, 1100), @(1440, 980))) {
    $t1 = [Diagnostics.Stopwatch]::StartNew()
    [Win32Perf]::SetWindowPos($h, [IntPtr]::Zero, 0, 0, $wh[0] + 16, $wh[1] + 39, 0x0002 -bor 0x0004) | Out-Null
    $okv = $false
    foreach ($i in 1..60) {
        Start-Sleep -Milliseconds 20
        $m = Invoke-Op 'map'
        if ($m -and [int]$m.viewport_w -eq $wh[0] -and [int]$m.viewport_h -eq $wh[1]) { $okv = $true; break }
    }
    $t1.Stop()
    $resizeMs += $t1.Elapsed.TotalMilliseconds
    if (-not $okv) { $results.resize_stuck = $true }
}
$results.resize_ack = Ms($resizeMs)
$script:Ops = @()

# ---- save + export ----
$null = Invoke-Op 'click' @('tab-canvas')
$p = Join-Path $Evidence ("perf-fixture-{0}.specimen.json" -f $Label)
$null = Invoke-Op 'type' @('field-path', $p)
$null = Invoke-Op 'key' @('Enter')
$msSave = @()
$t1 = [Diagnostics.Stopwatch]::StartNew(); $null = Invoke-Op 'click' @('save'); $t1.Stop(); $msSave += $t1.Elapsed.TotalMilliseconds
$png = Join-Path $Evidence ("perf-fixture-{0}.png" -f $Label)
$t1 = [Diagnostics.Stopwatch]::StartNew(); $null = Invoke-Op 'click' @('export'); $t1.Stop(); $msSave += $t1.Elapsed.TotalMilliseconds
$results.save_export = Ms($msSave)
$results.export_written = Test-Path $png
$script:Ops = @()

$tScenario.Stop()
$results.scenario_ms = [math]::Round($tScenario.Elapsed.TotalMilliseconds, 1)

# ---- final snapshot flush ----
$null = Invoke-Op 'snapshot' @((Join-Path $Evidence ("perf-final-{0}.png" -f $Label)))
Start-Sleep -Milliseconds 500
$b = Perf-Read
$results.perf_delta = Delta $a $b
$results.perf_final = $b
$results.op_failures = @($script:Ops | Where-Object { -not $_.ok }).Count

# ---- close ----
$p2 = Get-Process specimen -ErrorAction SilentlyContinue | Select-Object -First 1
if ($p2) { Stop-Process -Id $p2.Id -Force -ErrorAction SilentlyContinue }

$results | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutFile -Encoding utf8
Write-Output ("WROTE " + $OutFile)
$results | ConvertTo-Json -Depth 8
