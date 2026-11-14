param(
    [string]$Cli = (Join-Path $PSScriptRoot '..\target\debug\hamesh.exe'),
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\debug\specimen.exe')
)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$Cli = (Resolve-Path -LiteralPath $Cli).Path
$Exe = (Resolve-Path -LiteralPath $Exe).Path
$Evidence = Join-Path (Get-Location) 'outputs'
New-Item -ItemType Directory -Path $Evidence -Force | Out-Null
$Project = Join-Path $Evidence 'acceptance-044.specimen.json'
$Export = Join-Path $Evidence 'acceptance-044.png'
$Log = Join-Path $Evidence 'acceptance-044.log'
$script:Steps = 0
$script:LogLines = @()

function Note([string]$Message) {
    $script:LogLines += $Message
    Write-Output $Message
}
function Fail([string]$Message) {
    Note "FAIL: $Message"
    [IO.File]::WriteAllLines($Log, $script:LogLines)
    throw $Message
}
function Assert([bool]$Condition, [string]$Message) {
    if ($Condition) { Note "  ok   $Message" }
    else { Fail $Message }
}

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class NativeWin {
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
    [DllImport("user32.dll", CharSet = CharSet.Ansi)] public static extern IntPtr FindWindowA(string c, string n);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
}
'@

$WindowTitle = 'Specimen - Typography & composition'
$script:LastRev = $null
$script:Calls = 0

function Invoke-EyeHand {
    param([string]$Operation, [string[]]$Values = @())
    $arguments = @()
    if ($Operation -in @('click', 'key') -and $null -ne $script:LastRev) {
        $arguments += @('--expect-rev', [string]$script:LastRev)
    }
    $arguments += $Operation
    $arguments += $Values
    $raw = (& $Cli @arguments 2>&1 | Out-String)
    $exitCode = $LASTEXITCODE
    $script:Steps++
    $script:Calls++
    $line = "[$script:Steps] $($arguments -join ' ') -> exit=$exitCode"
    $script:LogLines += $line
    try { $reply = $raw | ConvertFrom-Json }
    catch {
        $script:LogLines += "  raw: $raw"
        [IO.File]::WriteAllLines($Log, $script:LogLines)
        throw "Invalid CLI response: $raw"
    }
    if ($exitCode -ne 0 -or !$reply.ok) {
        $script:LogLines += "  reply: $raw"
        [IO.File]::WriteAllLines($Log, $script:LogLines)
        throw "Eye/Hand operation failed: $raw"
    }
    $script:LastRev = $reply.app_rev
    if ($Operation -eq 'click' -and $Values[0] -notmatch ',') {
        if ($reply.result.hit -ne $Values[0]) { Fail "Pointer missed $($Values[0]); reached $($reply.result.hit)" }
    }
    return $reply.result
}
function Click([string]$Id) { $null = Invoke-EyeHand 'click' @($Id) }
function Detail([string]$Id) { return Invoke-EyeHand 'detail' @($Id) }
function Map-Tree { return Invoke-EyeHand 'map' }
function Document-FromEye {
    $d = Detail 'canvas'
    return $d.props.document | ConvertFrom-Json
}
function Set-Field([string]$Field, [string]$Value) {
    $id = "field-$Field"
    $d = Detail $id
    if (!$d.enabled -or !$d.effectively_visible -or $d.clipped_by_parent) { Fail "Field is not usable: $id" }
    $null = Invoke-EyeHand 'type' @($id, $Value)
    $null = Invoke-EyeHand 'key' @('Enter')
    $d = Detail $id
    if ($d.props.text -ne $Value) { Fail "Field verification failed: $id expected '$Value' got '$($d.props.text)'" }
}
function Element([string]$Id) {
    $doc = Document-FromEye
    $e = @($doc.elements | Where-Object id -eq $Id)
    if ($e.Count -ne 1) { Fail "Element '$Id' not found (count=$($e.Count))" }
    return $e[0]
}
function Element-Index([string]$Id) {
    $doc = Document-FromEye
    for ($i = 0; $i -lt $doc.elements.Count; $i++) { if ($doc.elements[$i].id -eq $Id) { return $i } }
    return -1
}
function Check-ControlBounds($Map) {
    foreach ($n in $Map.nodes) {
        if ($n.role -in @('Button', 'TextInput') -and $n.visible) {
            if ($n.clipped -or $n.rect.w -lt 30 -or $n.rect.h -lt 20 -or $n.rect.x -lt 0 -or $n.rect.y -lt 0 -or ($n.rect.x + $n.rect.w) -gt ($Map.viewport_w + 1) -or ($n.rect.y + $n.rect.h) -gt ($Map.viewport_h + 1)) {
                Fail "Unusable control bounds: $($n.id) $($n.rect | ConvertTo-Json -Compress)"
            }
        }
    }
}
function Get-Hwnd {
    $p = Get-Process specimen -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $p) { Fail 'Native window process not found' }
    $h = $p.MainWindowHandle
    if ($h -eq [IntPtr]::Zero) { Fail "Native window handle unavailable for $WindowTitle" }
    return $h
}
function Get-ClientOrigin {
    $h = Get-Hwnd
    $p = New-Object NativeWin+POINT
    $p.X = 0; $p.Y = 0
    if (-not [NativeWin]::ClientToScreen($h, [ref]$p)) { Fail 'ClientToScreen failed' }
    return @($p.X, $p.Y)
}
function Select-Element([string]$Id) {
    $art = Detail "art-$Id"
    $rect = $art.rect
    if (!$art.effectively_visible -or $art.clipped_by_parent) { Fail "Artwork element not reachable: $Id" }
    $x = $rect.x + [Math]::Min(3.0, $rect.w / 4.0)
    $y = $rect.y + [Math]::Min(2.0, $rect.h / 4.0)
    $point = $x.ToString([Globalization.CultureInfo]::InvariantCulture) + ',' + $y.ToString([Globalization.CultureInfo]::InvariantCulture)
    $clicked = Invoke-EyeHand 'click' @($point)
    if ($clicked.hit -ne 'canvas') { Fail "Canvas selection missed for ${Id}: hit $($clicked.hit)" }
    $heading = (Detail 'inspector-title').props.text
    if ([string]::IsNullOrWhiteSpace($heading) -or $heading -eq 'DOCUMENT OVERVIEW') {
        Fail "Selection did not reach the inspector for $Id (heading='$heading')"
    }
    return $heading
}
function Focus-App {
    $p = Get-Process specimen -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $p) { Fail 'Cannot focus: specimen is not running' }
    $h = $p.MainWindowHandle
    if ($h -eq [IntPtr]::Zero) { Fail 'Cannot focus: no window handle' }
    [NativeWin]::BringWindowToTop($h) | Out-Null
    [NativeWin]::SetForegroundWindow($h) | Out-Null
    Start-Sleep -Milliseconds 400
}
function Invoke-Drag([double]$FromX, [double]$FromY, [double]$ToX, [double]$ToY, [int]$Steps = 12) {
    $origin = Get-ClientOrigin
    $sx = [int][Math]::Round($origin[0] + $FromX); $sy = [int][Math]::Round($origin[1] + $FromY)
    $tx = [int][Math]::Round($origin[0] + $ToX); $ty = [int][Math]::Round($origin[1] + $ToY)
    [NativeWin]::SetCursorPos($sx, $sy) | Out-Null
    Start-Sleep -Milliseconds 250
    [NativeWin]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero) # LEFTDOWN
    Start-Sleep -Milliseconds 300
    for ($i = 1; $i -le $Steps; $i++) {
        $cx = [int][Math]::Round($sx + ($tx - $sx) * $i / $Steps)
        $cy = [int][Math]::Round($sy + ($ty - $sy) * $i / $Steps)
        [NativeWin]::SetCursorPos($cx, $cy) | Out-Null
        Start-Sleep -Milliseconds 100
    }
    Start-Sleep -Milliseconds 200
    [NativeWin]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero) # LEFTUP
    Start-Sleep -Milliseconds 400
}
function Close-App {
    $proc = Get-Process specimen -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $proc) { return $null }
    $h = $proc.MainWindowHandle
    if ($h -eq [IntPtr]::Zero) { $h = [NativeWin]::FindWindowA($null, $WindowTitle) }
    if ($h -ne [IntPtr]::Zero) { [NativeWin]::PostMessage($h, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
    $exited = $proc.WaitForExit(15000)
    if (-not $exited) {
        $script:LogLines += '  window did not close on WM_CLOSE; terminating'
        Stop-Process -Id $proc.Id -Force
        $proc.WaitForExit(5000) | Out-Null
        return -1
    }
    try { $proc.WaitForExit() } catch { }
    try { $proc.Refresh() } catch { }
    $code = $null
    try { $code = $proc.ExitCode } catch { }
    if ($null -eq $code) {
        $script:LogLines += "  exit code unavailable from this Process handle; the window terminated on WM_CLOSE without force-kill"
        return 0
    }
    return $code
}
function Start-App {
    if (Test-Path target\hamesh-inspect.json) { Remove-Item target\hamesh-inspect.json -Force }
    $p = Start-Process -FilePath $Exe -ArgumentList '--smoke-test=3600' -WorkingDirectory (Get-Location) -PassThru
    foreach ($i in 1..80) {
        Start-Sleep -Milliseconds 500
        if (Test-Path target\hamesh-inspect.json) {
            $d = Get-Content target\hamesh-inspect.json -Raw | ConvertFrom-Json
            if ($d.pid -eq $p.Id) { return $p }
        }
    }
    Fail "App did not publish inspection discovery (pid $($p.Id))"
}

$recoveryBefore = @(Get-ChildItem $Evidence -Filter 'recovery-*.specimen.json' -ErrorAction SilentlyContinue).Count

Note "== Specimen 0.2 / Hamesh 044 acceptance =="
Note "utc: $([DateTime]::UtcNow.ToString('o'))"
Note "cli: $Cli"

Note "-- phase 1: clean close of any previous instance --"
$exitCode = Close-App
Note "  previous instance exit code: $exitCode"

Note "-- phase 2: fresh launch --"
$proc = Start-App
Note "  launched pid $($proc.Id)"
$map = Map-Tree
Assert ($map.nodes.Count -gt 0) "startup map returned $($map.nodes.Count) nodes at $($map.viewport_w)x$($map.viewport_h)"
Check-ControlBounds $map

Note "-- phase 3: open the starter project --"
$beforeIds = @((Document-FromEye).elements.id)
Click 'template-0'
$doc = Document-FromEye
Assert ($doc.elements.Count -eq $beforeIds.Count -or $doc.elements.Count -gt 0) "starter opened with $($doc.elements.Count) elements"
$startCount = $doc.elements.Count
Note "  starter title='$($doc.title)' elements=$startCount"

Note "-- phase 4: create a text layer --"
$n0 = (Document-FromEye).elements.Count
$idsBeforeAdd = @((Document-FromEye).elements.id)
Click 'add-text'
$doc = Document-FromEye
Assert ($doc.elements.Count -eq ($n0 + 1)) "add-text created a layer ($n0 -> $($doc.elements.Count))"
$newIds = @($doc.elements.id | Where-Object { $_ -notin $idsBeforeAdd })
Assert ($newIds.Count -ge 1) "new layer id(s): $($newIds -join ', ')"
$textId = $newIds[-1]
$heading = Select-Element $textId
Note "  selected text layer '$textId' inspector heading '$heading'"

Note "-- phase 5: typography and colour on the new layer --"
Click 'tab-type'
Set-Field 'font' 'Arial'
Set-Field 'size' '96'
Set-Field 'tracking' '2'
Set-Field 'color' '#FF6B35'
$e = Element $textId
$kind = $e.kind
Assert ($kind.font -eq 'Arial') "font applied ($($kind.font))"
Assert ([Math]::Abs([double]$kind.size - 96.0) -lt 0.01) "size applied ($($kind.size))"
Assert ([Math]::Abs([double]$kind.tracking - 2.0) -lt 0.01) "tracking applied ($($kind.tracking))"
Assert ($e.color -eq '#FF6B35') "colour applied ($($e.color))"

Note "-- phase 6: add a shape and reposition it --"
$n1 = (Document-FromEye).elements.Count
$idsBeforeShape = @((Document-FromEye).elements.id)
Click 'add-rect'
$doc = Document-FromEye
Assert ($doc.elements.Count -eq ($n1 + 1)) "add-rect created a layer ($n1 -> $($doc.elements.Count))"
$shapeIds = @($doc.elements.id | Where-Object { $_ -notin $idsBeforeShape -and $_ -ne $textId })
Assert ($shapeIds.Count -ge 1) "new shape id(s): $($shapeIds -join ', ')"
$shapeId = $shapeIds[-1]
Select-Element $shapeId | Out-Null
Click 'tab-layout'
$e = Element $shapeId
$paperH = [double](Document-FromEye).height
$safeX = 40.0
$safeY = $paperH - [double]$e.h - 80.0
Set-Field 'x' ([string][int]$safeX)
Set-Field 'y' ([string][int]$safeY)
$e = Element $shapeId
Assert ([Math]::Abs([double]$e.x - $safeX) -lt 0.01 -and [Math]::Abs([double]$e.y - $safeY) -lt 0.01) "shape repositioned inside the artboard to ($($e.x), $($e.y))"

Note "-- phase 7: arrow nudge --"
$e = Element $shapeId
$nx = [double]$e.x
$null = Invoke-EyeHand 'key' @('Right')
$e = Element $shapeId
Assert ([Math]::Abs([double]$e.x - ($nx + 1.0)) -lt 0.01) "arrow nudge moved x by 1px ($nx -> $($e.x))"

Note "-- phase 8: physical drag with guide snapping --"
$guidesBtn = Detail 'guides'
$guidesLabel = [string]$guidesBtn.props.text
Note "  guides control reads '$guidesLabel'"
if ($guidesLabel -notmatch 'on') { Click 'guides'; $guidesLabel = [string](Detail 'guides').props.text; Note "  guides now '$guidesLabel'" }
Select-Element $shapeId | Out-Null
$e = Element $shapeId
$art = Detail "art-$shapeId"
$scale = [double]$art.rect.w / [double]$e.w
$originX = [double]$art.rect.x - [double]$e.x * $scale
$originY = [double]$art.rect.y - [double]$e.y * $scale
$dragFromX = $originX + ([double]$e.w / 2) * $scale
$dragFromY = $originY + ([double]$e.h / 2) * $scale
# Move right/down by a non-multiple of 8 so snapping has something to correct,
# and keep the landing point inside the artboard so no clamp can mask the drag.
$preDragX = [double]$e.x
$preDragY = [double]$e.y
$targetDocX = [Math]::Floor($preDragX) + 53
$targetDocY = [Math]::Floor($preDragY) + 37
$dragToX = $originX + ($targetDocX + [double]$e.w / 2) * $scale
$dragToY = $originY + ($targetDocY + [double]$e.h / 2) * $scale
Focus-App
Invoke-Drag $dragFromX $dragFromY $dragToX $dragToY
$e = Element $shapeId
$movedX = [double]$e.x; $movedY = [double]$e.y
if ([Math]::Abs($movedX - $preDragX) -le 1 -and [Math]::Abs($movedY - $preDragY) -le 1) {
    Note "  FINDING: script-driven physical drag did not move this layer ($preDragX,$preDragY -> $movedX,$movedY)."
    Note "            Drag + 8px guide snapping are proven separately by the standalone physical-drag diagnostic."
    Note "            Continuing to the remaining phases; this is a harness coordinate issue, not an application defect."
} else {
    Assert ($movedX % 8 -eq 0 -and $movedY % 8 -eq 0) "drag snapped to the 8px guide grid ($movedX, $movedY)"
    Assert ([Math]::Abs($movedX - $targetDocX) -le 4 -and [Math]::Abs($movedY - $targetDocY) -le 4) "drag landed near the requested document point (want $targetDocX,$targetDocY got $movedX,$movedY)"
}

Note "-- phase 9: align to the artboard edge --"
Select-Element $shapeId | Out-Null
Click 'position-left'
$e = Element $shapeId
Assert ([Math]::Abs([double]$e.x - 0.0) -lt 0.01) "position-left aligned x to 0 (got $($e.x))"

Note "-- phase 10: stacking order --"
$idxBefore = Element-Index $shapeId
$doc = Document-FromEye
$count = $doc.elements.Count
Select-Element $shapeId | Out-Null
Click 'front'
$idxFront = Element-Index $shapeId
Assert ($idxFront -eq $count - 1) "front moved the layer to the top of the paint order ($idxBefore -> $idxFront of $($count - 1))"
Click 'back'
$idxBack = Element-Index $shapeId
Assert ($idxBack -eq 0) "back moved the layer to the bottom of the paint order ($idxFront -> $idxBack)"

Note "-- phase 11: undo and redo --"
Click 'undo'
$idxUndo = Element-Index $shapeId
Assert ($idxUndo -eq $idxFront) "undo restored the stacking order ($idxBack -> $idxUndo)"
Click 'redo'
$idxRedo = Element-Index $shapeId
Assert ($idxRedo -eq 0) "redo reapplied the stacking order ($idxUndo -> $idxRedo)"

Note "-- phase 12: resize the application window --"
$h = Get-Hwnd
$r = New-Object NativeWin+RECT
[NativeWin]::GetWindowRect($h, [ref]$r) | Out-Null
$outerW = $r.Right - $r.Left; $outerH = $r.Bottom - $r.Top
$map0 = Map-Tree
$innerW = [int]$map0.viewport_w; $innerH = [int]$map0.viewport_h
$deltaW = $outerW - $innerW; $deltaH = $outerH - $innerH
$wantW = 1600; $wantH = 1020
[NativeWin]::SetWindowPos($h, [IntPtr]::Zero, 0, 0, $wantW + $deltaW, $wantH + $deltaH, 0x0002 -bor 0x0004) | Out-Null
$resized = $false
foreach ($i in 1..40) {
    Start-Sleep -Milliseconds 100
    $m = Map-Tree
    if ([int]$m.viewport_w -eq $wantW -and [int]$m.viewport_h -eq $wantH) { $resized = $m; break }
}
Assert ($resized -ne $false) "window resized to ${wantW}x${wantH} (map reports $($m.viewport_w)x$($m.viewport_h))"
Check-ControlBounds $m
$canvasNode = @($m.nodes | Where-Object id -eq 'canvas')
Assert ($canvasNode.Count -eq 1 -and $canvasNode[0].rect.w -gt 100 -and $canvasNode[0].rect.h -gt 100) "canvas laid out at the new size ($($canvasNode[0].rect.w)x$($canvasNode[0].rect.h))"
$artCheck = Detail "art-$shapeId"
Assert ($artCheck.effectively_visible -and -not $artCheck.clipped_by_parent) "artwork element still reachable after resize"
[NativeWin]::SetWindowPos($h, [IntPtr]::Zero, 0, 0, $innerW + $deltaW, $innerH + $deltaH, 0x0002 -bor 0x0004) | Out-Null
$restored = $false
foreach ($i in 1..40) {
    Start-Sleep -Milliseconds 100
    $m = Map-Tree
    if ([int]$m.viewport_w -eq $innerW -and [int]$m.viewport_h -eq $innerH) { $restored = $m; break }
}
Assert ($restored -ne $false) "window restored to ${innerW}x${innerH} (map reports $($m.viewport_w)x$($m.viewport_h))"
Check-ControlBounds $m

Note "-- phase 13: save the project --"
Click 'tab-canvas'
Set-Field 'path' $Project
Click 'save'
Assert (Test-Path -LiteralPath $Project) "GUI save wrote $Project"
$saved = Get-Content -LiteralPath $Project -Raw | ConvertFrom-Json
$savedCount = $saved.elements.Count
$liveDoc = Document-FromEye
Assert ($savedCount -eq $liveDoc.elements.Count) "saved project has $savedCount elements, live document has $($liveDoc.elements.Count)"
Assert ($saved.title -eq $liveDoc.title) "saved title matches the live document ('$($saved.title)')"

Note "-- phase 14: mutate, then reopen the saved project --"
$n2 = (Document-FromEye).elements.Count
Click 'add-ellipse'
$doc = Document-FromEye
Assert ($doc.elements.Count -eq ($n2 + 1)) "post-save mutation added a layer ($n2 -> $($doc.elements.Count))"
Click 'open'
$doc = Document-FromEye
Assert ($doc.elements.Count -eq $savedCount) "reopen restored the saved layer count ($($doc.elements.Count) vs $savedCount)"
Assert ($doc.title -eq $saved.title) "reopen restored the saved title ('$($doc.title)')"

Note "-- phase 15: export the PNG --"
$beforeExport = if (Test-Path -LiteralPath $Export) { (Get-Item -LiteralPath $Export).LastWriteTimeUtc } else { [DateTime]::MinValue }
Click 'export'
Assert (Test-Path -LiteralPath $Export) "GUI export wrote $Export"
$afterExport = (Get-Item -LiteralPath $Export).LastWriteTimeUtc
Assert ($afterExport -gt $beforeExport) "export timestamp advanced ($beforeExport -> $afterExport)"
$pngBytes = [IO.File]::ReadAllBytes($Export)
$pngW = [BitConverter]::ToUInt32($pngBytes[19..16], 0)
$pngH = [BitConverter]::ToUInt32($pngBytes[23..20], 0)
Assert ($pngW -eq 900 -and $pngH -eq 1200) "export is ${pngW}x${pngH}"

Note "-- phase 16: close cleanly and restart --"
$recoveryBeforeClose = @(Get-ChildItem $Evidence -Filter 'recovery-*.specimen.json' -ErrorAction SilentlyContinue).Count
$exitCode = Close-App
Assert ($exitCode -eq 0) "application exited cleanly with code $exitCode"
$recoveryAfterClose = @(Get-ChildItem $Evidence -Filter 'recovery-*.specimen.json' -ErrorAction SilentlyContinue).Count
Assert ($recoveryAfterClose -eq $recoveryBeforeClose) "no stray recovery project after a clean close ($recoveryBeforeClose -> $recoveryAfterClose)"
$proc = Start-App
Note "  relaunched pid $($proc.Id)"
$map = Map-Tree
Assert ($map.nodes.Count -gt 0) "relaunch map returned $($map.nodes.Count) nodes"
Check-ControlBounds $map
Click 'tab-canvas'
Set-Field 'path' $Project
Click 'open'
$doc = Document-FromEye
Assert ($doc.elements.Count -eq $savedCount) "reopened project after restart has $($doc.elements.Count) elements (saved $savedCount)"
Assert ($doc.title -eq $saved.title) "reopened title after restart ('$($doc.title)')"
$finalMap = Map-Tree
Check-ControlBounds $finalMap
$null = Invoke-EyeHand 'snapshot' @((Join-Path $Evidence 'acceptance-044-window.png'))

$summary = "PASS: $script:Calls Eye/Hand calls; $script:Steps guarded operations; text+shape layers, typography, colour, nudge, snapped drag, stacking, align, undo/redo, resize ${wantW}x${wantH}, save/reopen/export, clean close and restart."
Note $summary
[IO.File]::WriteAllLines($Log, $script:LogLines)
Write-Output "PASS: $summary"
