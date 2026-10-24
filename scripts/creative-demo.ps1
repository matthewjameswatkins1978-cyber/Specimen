param(
    [string]$Cli = (Join-Path $PSScriptRoot '..\target\debug\hamesh.exe')
)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$Cli = (Resolve-Path -LiteralPath $Cli).Path
$Evidence = Join-Path (Get-Location) 'outputs'
New-Item -ItemType Directory -Path $Evidence -Force | Out-Null
$Log = Join-Path $Evidence 'creative-interactions.jsonl'
if (Test-Path -LiteralPath $Log) {
    Copy-Item -LiteralPath $Log -Destination ($Log + '.' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.previous')
}
[IO.File]::WriteAllText($Log, '')
$script:LastRev = $null
$script:Calls = 0

function Invoke-EyeHand {
    param([string]$Operation, [string[]]$Values = @())
    $arguments = @()
    if ($Operation -in @('click','key') -and $null -ne $script:LastRev) {
        $arguments += @('--expect-rev', [string]$script:LastRev)
    }
    $arguments += $Operation
    $arguments += $Values
    $raw = (& $Cli @arguments 2>&1 | Out-String)
    $exitCode = $LASTEXITCODE
    $script:Calls++
    try { $reply = $raw | ConvertFrom-Json }
    catch {
        [IO.File]::AppendAllText($Log, ((@{ sequence=$script:Calls; arguments=$arguments; exit=$exitCode; raw=$raw } | ConvertTo-Json -Compress -Depth 60) + "`n"))
        throw "Invalid CLI response: $raw"
    }
    [IO.File]::AppendAllText($Log, ((@{ sequence=$script:Calls; utc=[DateTime]::UtcNow.ToString('o'); arguments=$arguments; exit=$exitCode; reply=$reply } | ConvertTo-Json -Compress -Depth 60) + "`n"))
    if ($exitCode -ne 0 -or !$reply.ok) { throw "Eye/Hand operation failed: $raw" }
    $script:LastRev = $reply.app_rev
    if ($Operation -eq 'click' -and $Values[0] -notmatch ',') {
        if ($reply.result.hit -ne $Values[0]) {
            throw "Pointer missed $($Values[0]); reached $($reply.result.hit)"
        }
    }
    return $reply.result
}
function Click([string]$Id) { $null = Invoke-EyeHand 'click' @($Id) }
function Detail([string]$Id) { return Invoke-EyeHand 'detail' @($Id) }
function Set-Field([string]$Field, [string]$Value) {
    $id = "field-$Field"
    $d = Detail $id
    if (!$d.enabled -or !$d.effectively_visible -or $d.clipped_by_parent) {
        throw "Field is not usable: $id"
    }
    $null = Invoke-EyeHand 'type' @($id, $Value)
    $null = Invoke-EyeHand 'key' @('Enter')
    $d = Detail $id
    if ($d.props.text -ne $Value) { throw "Field verification failed: $id expected $Value got $($d.props.text)" }
}
function Document-FromEye {
    $d = Detail 'canvas'
    return $d.props.document | ConvertFrom-Json
}
function Select-Artwork([string]$Id) {
    $art = Detail "art-$Id"
    $element = $art.props.element | ConvertFrom-Json
    if (!$art.effectively_visible -or $art.clipped_by_parent) { throw "Artwork element not fully reachable: $Id" }
    # A real canvas click near the box's top-left avoids intersecting later
    # text layers. Read-only Eye ghost nodes are never invoked as controls.
    $x = $art.rect.x + [Math]::Min(3.0, $art.rect.w / 4.0)
    $y = $art.rect.y + [Math]::Min(2.0, $art.rect.h / 4.0)
    $point = $x.ToString('R', [Globalization.CultureInfo]::InvariantCulture) + ',' + $y.ToString('R', [Globalization.CultureInfo]::InvariantCulture)
    $clicked = Invoke-EyeHand 'click' @($point)
    if ($clicked.hit -ne 'canvas') { throw "Canvas selection missed: $($clicked.hit)" }
    $inspector = Detail 'inspector-title'
    $heading = $inspector.props.text
    $want = $element.name
    if ($element.kind.type -eq 'text') {
        $first = (($element.kind.text -split "`n")[0]).Trim()
        if ($first.Length -gt 24) { $first = $first.Substring(0, 24) }
        $want = if ($first) { "Text · $first" } else { "Text" }
    }
    if ($heading -ne $want) { throw "Wrong element selected: wanted $want, inspector shows $heading" }
    Click 'tab-type'
}
function Check-ControlBounds($Map) {
    foreach ($n in $Map.nodes) {
        if ($n.role -in @('Button','TextInput') -and $n.visible) {
            if ($n.clipped -or $n.rect.w -lt 30 -or $n.rect.h -lt 20 -or $n.rect.x -lt 0 -or $n.rect.y -lt 0 -or ($n.rect.x+$n.rect.w) -gt ($Map.viewport_w+1) -or ($n.rect.y+$n.rect.h) -gt ($Map.viewport_h+1)) {
                throw "Unusable control bounds: $($n.id) $($n.rect | ConvertTo-Json -Compress)"
            }
        }
    }
}

# External operator mission: create a retro-futurist electronic music poster.
$map = Invoke-EyeHand 'map'
Check-ControlBounds $map
Click 'template-0'
Click 'tab-canvas'
Set-Field 'title' 'After Frequency / Halloween transmission'
$project = Join-Path $Evidence 'after-frequency.specimen.json'
Set-Field 'path' $project

Select-Artwork 'title'
Set-Field 'font' 'Arial'
Set-Field 'size' '142'
Set-Field 'tracking' '1.5'
Select-Artwork 'subtitle'
Set-Field 'text' 'FREQUENCY'
Set-Field 'tracking' '1'
Select-Artwork 'venue'
Set-Field 'text' 'THE CONCRETE HALL  /  BRISTOL'
Select-Artwork 'date'
Set-Field 'text' 'SATURDAY 31 OCTOBER'
Select-Artwork 'lineup'
Set-Field 'text' 'MIRA  +  PHASE ARRAY  +  STATIC GARDEN'
Select-Artwork 'coordinates'
Set-Field 'text' "51.4545 N`n02.5879 W"
Select-Artwork 'orbit-lime'
Set-Field 'color' '#D6F04A'
Select-Artwork 'genre'
Set-Field 'color' '#D6F04A'

# Explore an inset, inspect its measured relation to the shared poster grid,
# and repair the resulting mismatch through the visible geometry inspector.
Click 'tab-layout'
$before = Document-FromEye
$rule = @($before.elements | Where-Object id -eq 'top-rule')[0]
$trialX = [single]$rule.x + 40
Set-Field 'x' ([string]$trialX)
$trial = Document-FromEye
$genreTrial = @($trial.elements | Where-Object id -eq 'genre')[0]
$mismatch = [single]$genreTrial.x - [single]$rule.x
if ([Math]::Abs($mismatch) -lt 1) { throw 'Trial did not produce a measurable alignment mismatch' }
$null = Invoke-EyeHand 'snapshot' @((Join-Path $Evidence 'alignment-trial.png'))
Set-Field 'x' ([string]$rule.x)
$corrected = Document-FromEye
$genreCorrected = @($corrected.elements | Where-Object id -eq 'genre')[0]
if ([Math]::Abs([single]$genreCorrected.x - [single]$rule.x) -gt 0.01) { throw 'Alignment correction did not apply' }
$genreEye = Detail 'art-genre'
$ruleEye = Detail 'art-top-rule'
if ([Math]::Abs($genreEye.rect.x - $ruleEye.rect.x) -gt 0.01) { throw 'Rendered Eye geometry differs after correction' }

# The initial starter's headline boxes use optical insets (40/42) rather
# than the support copy's 48 px grid. Choose a strict shared left edge,
# measure the actual mismatch, then inspect the live repaired rectangles.
$headlineBefore = Document-FromEye
$titleBefore = @($headlineBefore.elements | Where-Object id -eq 'title')[0]
$subtitleBefore = @($headlineBefore.elements | Where-Object id -eq 'subtitle')[0]
$headlineMismatch = [single]$titleBefore.x - [single]$rule.x
$subtitleMismatch = [single]$subtitleBefore.x - [single]$rule.x
Select-Artwork 'title'
Click 'tab-layout'
Set-Field 'x' ([string]$rule.x)
Select-Artwork 'subtitle'
Click 'tab-layout'
Set-Field 'x' ([string]$rule.x)
$titleEye = Detail 'art-title'
$subtitleEye = Detail 'art-subtitle'
$ruleEye = Detail 'art-top-rule'
if ([Math]::Abs($titleEye.rect.x-$ruleEye.rect.x) -gt 0.01 -or [Math]::Abs($subtitleEye.rect.x-$ruleEye.rect.x) -gt 0.01) { throw 'Headline alignment correction failed' }

# Clear guides and selection, then save/export through ordinary GUI controls.
Click 'guides'
$null = Invoke-EyeHand 'key' @('Escape')
Click 'save'
Click 'export'
$null = Invoke-EyeHand 'snapshot' @((Join-Path $Evidence 'final-specimen.png'))
$null = Invoke-EyeHand 'snapshot' @((Join-Path $Evidence 'final-artwork-view.png'))
Select-Artwork 'title'
$null = Invoke-EyeHand 'snapshot' @((Join-Path $Evidence 'final-specimen.png'))
$finalMap = Invoke-EyeHand 'map'
Check-ControlBounds $finalMap
$finalDoc = Document-FromEye
if ($finalDoc.title -ne 'After Frequency / Halloween transmission') { throw 'Unexpected final document title' }
if (!(Test-Path -LiteralPath $project)) { throw 'GUI Save did not produce a project file' }
$png = Join-Path $Evidence 'after-frequency.png'
if (!(Test-Path -LiteralPath $png)) { throw 'GUI Export did not produce expected PNG' }

$report = @"
# Independent creative GUI operation

Result: automated GUI operations and geometry checks passed. Visual review of the exported PNG and final application frame is recorded separately by the operating agent.

The agent started with the After Frequency template using its ordinary library button, retained the cream/charcoal orbital composition, selected Arial typography, adjusted tracking, introduced acid lime, and changed the event to a fictional Bristol Halloween transmission featuring MIRA, Phase Array and Static Garden.

Every document change used the external Hamesh CLI, actual canvas hit-testing, focused inspector fields, Enter commits, or ordinary Save/Export buttons. The script did not write a document file or invoke direct document mutation/render commands.

A tested inset placed the genre line $mismatch document pixels to the right of the shared rule grid. Eye document and rendered geometry exposed the mismatch. The agent corrected X through the inspector and verified that the two Eye rectangles now share the same left edge. This was an explicit composition trial and correction, not a claim that the initial starter had a hidden error.

Evidence: creative-interactions.jsonl contains $($script:Calls) complete CLI requests/replies; alignment-trial.png shows the trial; final-specimen.png shows the final application frame; after-frequency.specimen.json and after-frequency.png were written by the GUI.

Click and key operations used current revision guards. The pinned upstream type operation has no revision guard; it still focused by real pointer hit and shared the normal Composer input path. No human visual debugging or creative intervention was requested during this run. CPU rendering only; no GPU parity claim. A separate canonical re-render comparison remains the main agent's responsibility.
"@
[IO.File]::WriteAllText((Join-Path $Evidence 'creative-agent-report.md'), $report)
Write-Output "PASS: $($script:Calls) external Eye/Hand calls; measured/corrected $mismatch px alignment trial; GUI project and PNG saved."
