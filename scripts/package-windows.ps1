param(
    [string]$OutputDirectory = 'dist/d2bot-windows-x64',
    [switch]$SkipBuild,
    [switch]$IncludeDevelopmentAssets
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$destination = [IO.Path]::GetFullPath((Join-Path $projectRoot $OutputDirectory))
$prefix = $projectRoot.TrimEnd('\') + '\'
if (-not $destination.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Package destination must stay inside this checkout.' }
if (Test-Path -LiteralPath $destination) { throw "Refusing to replace existing package: $destination" }
$cargo = Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe'
if (-not (Test-Path -LiteralPath $cargo)) { $cargo = (Get-Command cargo -ErrorAction Stop).Source }
$target = 'x86_64-pc-windows-msvc'
$linkConfiguration = 'target.x86_64-pc-windows-msvc.rustflags=["-C","target-feature=+crt-static"]'
$temporaryRoot = Join-Path $projectRoot 'target/tmp'
[IO.Directory]::CreateDirectory($temporaryRoot) | Out-Null
$stage = Join-Path $temporaryRoot ('package-stage-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($stage) | Out-Null
$published = $false
Push-Location $projectRoot
try {
    if (-not $SkipBuild) {
        & $cargo build --release --locked --target $target --config $linkConfiguration --bin d2bot --example runtime-assets-check
        if ($LASTEXITCODE -ne 0) { throw 'Portable release build failed.' }
    }
    $binary = Join-Path $projectRoot "target/$target/release/d2bot.exe"
    if (-not (Test-Path -LiteralPath $binary)) { throw 'Portable release binary missing.' }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $dumpbins = @(& $vswhere -latest -products '*' -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe')
    if ($LASTEXITCODE -ne 0 -or $dumpbins.Count -eq 0) { throw 'MSVC dumpbin is required to verify actual imports.' }
    $dependencyText = ((& $dumpbins[0] /DEPENDENTS $binary) -join "`n")
    if ($LASTEXITCODE -ne 0) { throw 'PE import inspection failed.' }
    $imports = @([regex]::Matches($dependencyText, '(?im)^\s+([a-z0-9_.-]+\.dll)\s*$') | ForEach-Object { $_.Groups[1].Value.ToLowerInvariant() } | Sort-Object -Unique)
    $systemDlls = @('kernel32.dll','user32.dll','gdi32.dll','dwrite.dll','oleaut32.dll','uiautomationcore.dll','combase.dll','dwmapi.dll','shell32.dll','ole32.dll','bcryptprimitives.dll','opengl32.dll','advapi32.dll','imm32.dll','uxtheme.dll','ntdll.dll')
    if ($imports.Count -eq 0 -or @($imports | Where-Object { $_ -notin $systemDlls -and $_ -notmatch '^(api|ext)-ms-win-' }).Count -gt 0 -or @($imports | Where-Object { $_ -match '(vcruntime|msvcp|ms-win-crt-|python|opencv|ffmpeg|xvid|libav)' }).Count -gt 0) { throw "Unexpected portable runtime DLL dependency: $($imports -join ', ')" }
    $metadataText = & $cargo metadata --locked --format-version 1 --filter-platform $target
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed.' }
    $metadata = ($metadataText -join "`n") | ConvertFrom-Json -Depth 100
    $packages = @{}; foreach ($package in $metadata.packages) { $packages[$package.id] = $package }
    $nodes = @{}; foreach ($node in $metadata.resolve.nodes) { $nodes[$node.id] = $node }
    $reachable = [Collections.Generic.HashSet[string]]::new()
    $queue = [Collections.Generic.Queue[string]]::new(); $queue.Enqueue($metadata.resolve.root)
    while ($queue.Count -gt 0) {
        $id = $queue.Dequeue()
        if (-not $reachable.Add($id)) { continue }
        foreach ($dependency in $nodes[$id].deps) {
            if (@($dependency.dep_kinds | Where-Object { $null -eq $_.kind }).Count -gt 0) { $queue.Enqueue($dependency.pkg) }
        }
    }
    Copy-Item -LiteralPath $binary -Destination (Join-Path $stage 'd2bot.exe')
    foreach ($owner in @('config', 'licenses')) { Copy-Item -LiteralPath (Join-Path $projectRoot $owner) -Destination (Join-Path $stage $owner) -Recurse }
    $assetDestination = Join-Path $stage 'assets'; [IO.Directory]::CreateDirectory($assetDestination) | Out-Null
    $assetRoot = Join-Path $projectRoot 'assets'
    $assetPolicy = Get-Content -LiteralPath (Join-Path $assetRoot 'runtime-bundle.json') -Raw | ConvertFrom-Json -Depth 15
    if ($assetPolicy.schema -ne 1) { throw 'Unsupported runtime asset policy.' }
    $runtimeReferences = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in $assetPolicy.runtime_references) {
        $relative = $entry.path.Replace('\','/')
        $resolved = [IO.Path]::GetFullPath((Join-Path $assetRoot $relative))
        if (-not $resolved.StartsWith($assetRoot.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $resolved -PathType Leaf)) { throw "Invalid required runtime reference: $relative" }
        $runtimeReferences.Add([IO.Path]::GetRelativePath($assetRoot,$resolved).Replace('\','/')) | Out-Null
    }
    # 사용자 설정이 추가한 실행 자산도 제외 규칙보다 먼저 보존한다.
    function Add-ConfiguredAssets($value) {
        if ($value -is [string]) {
            $relative = $value.Replace('\','/')
            if ($relative.StartsWith('assets/',[StringComparison]::OrdinalIgnoreCase)) {
                $relative = $relative.Substring(7)
                $resolved = [IO.Path]::GetFullPath((Join-Path $assetRoot $relative))
                if (-not $resolved.StartsWith($assetRoot.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase) -or 'private' -in $relative.Split('/') -or -not (Test-Path -LiteralPath $resolved -PathType Leaf)) { throw "Configured asset is not portable inside shared assets: $value" }
                $runtimeReferences.Add([IO.Path]::GetRelativePath($assetRoot,$resolved).Replace('\','/')) | Out-Null
            }
        } elseif ($value -is [Collections.IDictionary]) {
            foreach ($child in $value.Values) { Add-ConfiguredAssets $child }
        } elseif ($value -is [Collections.IEnumerable]) {
            foreach ($child in $value) { Add-ConfiguredAssets $child }
        }
    }
    foreach ($configFile in Get-ChildItem -LiteralPath (Join-Path $projectRoot 'config') -Recurse -File -Filter '*.json') {
        Add-ConfiguredAssets (Get-Content -LiteralPath $configFile.FullName -Raw | ConvertFrom-Json -AsHashtable -Depth 100)
    }
    foreach ($item in Get-ChildItem -LiteralPath $assetRoot -Recurse -File -Force) {
        $relative = [IO.Path]::GetRelativePath($assetRoot,$item.FullName).Replace('\','/')
        $directories = @($relative.Split('/') | Select-Object -SkipLast 1)
        if ('private' -in $directories) { continue }
        if (-not $IncludeDevelopmentAssets -and @($directories | Where-Object { $_ -in $assetPolicy.excluded_directory_names }).Count -gt 0 -and -not $runtimeReferences.Contains($relative)) { continue }
        $copyTarget = Join-Path $assetDestination $relative
        [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($copyTarget)) | Out-Null
        Copy-Item -LiteralPath $item.FullName -Destination $copyTarget
    }
    $assetCheck = Join-Path $projectRoot "target/$target/release/examples/runtime-assets-check.exe"
    if (-not (Test-Path -LiteralPath $assetCheck -PathType Leaf)) { throw 'Build runtime-assets-check before packaging with -SkipBuild.' }
    & $assetCheck $stage
    if ($LASTEXITCODE -ne 0) { throw 'Required runtime assets failed to initialize in the staged package.' }
    Copy-Item -LiteralPath (Join-Path $projectRoot 'THIRD-PARTY-NOTICES.md') -Destination $stage
    $licenseRoot = Join-Path $stage 'licenses/cargo'; [IO.Directory]::CreateDirectory($licenseRoot) | Out-Null
    $licenseIndex = [Collections.Generic.List[object]]::new()
    $upstreamRoot = Join-Path $projectRoot 'licenses/cargo-upstream'
    $upstreamSources = Get-Content -LiteralPath (Join-Path $upstreamRoot 'sources.json') -Raw | ConvertFrom-Json -Depth 10
    foreach ($id in $reachable | Sort-Object) {
        if ($id -eq $metadata.resolve.root) { continue }
        $package = $packages[$id]
        $source = [IO.Path]::GetDirectoryName($package.manifest_path)
        $files = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        foreach ($file in Get-ChildItem -LiteralPath $source -File) {
            if ($file.Name -match '^(licen[cs]e|copying|notice|unlicense|copyright|ofl)([.\-_]|$)') { $files.Add($file.FullName) | Out-Null }
        }
        if ($null -ne $package.license_file) {
            $file = [IO.Path]::GetFullPath((Join-Path $source $package.license_file))
            if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "Declared license file missing for $($package.name)" }
            $files.Add($file) | Out-Null
        }
        # 내장 폰트의 별도 고지는 crate 루트 라이선스와 함께 보존한다.
        if ($package.name -eq 'epaint_default_fonts') {
            foreach ($file in Get-ChildItem -LiteralPath $source -File -Recurse) {
                if ($file.Name -match '(licen[cs]e|copying|notice|copyright|ofl|\.txt$)' -and $file.Extension -ne '.rs') { $files.Add($file.FullName) | Out-Null }
            }
        }
        $supplements = @($upstreamSources | Where-Object { "$($package.name)@$($package.version)" -in $_.packages })
        if ($files.Count -eq 0 -and $supplements.Count -eq 0) { throw "No local upstream notice found for $($package.name) $($package.version)" }
        $relativeFiles = [Collections.Generic.List[string]]::new()
        foreach ($file in $files | Sort-Object) {
            $relative = [IO.Path]::GetRelativePath($source, $file)
            if ($relative.StartsWith('..')) { throw "License escapes crate source: $($package.name)" }
            $relativeOutput = "$($package.name)-$($package.version)/$relative"
            $output = Join-Path $licenseRoot $relativeOutput
            [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($output)) | Out-Null
            Copy-Item -LiteralPath $file -Destination $output
            $relativeFiles.Add($relativeOutput.Replace('\', '/'))
        }
        foreach ($supplement in $supplements) {
            foreach ($file in $supplement.files) {
                $original = Join-Path $upstreamRoot $file.path
                if ((Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash.ToLowerInvariant() -cne $file.sha256) { throw "Pinned upstream notice changed: $($file.path)" }
                $relativeOutput = "$($package.name)-$($package.version)/upstream/$([IO.Path]::GetFileName($file.path))"
                $output = Join-Path $licenseRoot $relativeOutput
                [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($output)) | Out-Null
                Copy-Item -LiteralPath $original -Destination $output
                $relativeFiles.Add($relativeOutput)
            }
        }
        $licenseIndex.Add([ordered]@{ name=$package.name; version=$package.version; license=$package.license; repository=$package.repository; notices=@($relativeFiles) })
    }
    ConvertTo-Json -InputObject @($licenseIndex.ToArray()) -Depth 10 | Set-Content -LiteralPath (Join-Path $licenseRoot 'index.json') -Encoding utf8
    @'
# Diablo2 Rust — Windows x64

Keep d2bot.exe, config/, assets/ and the license files together.
Open d2bot.exe to launch the control panel. Its sibling package-manifest.json selects
default config and assets beside the executable, including when launched from another folder.
Explicit config/input/output paths and relative recording paths use the caller's folder.
From this package folder, terminal examples are:

    .\d2bot.exe
    .\d2bot.exe --help
    .\d2bot.exe --diagnose
    .\d2bot.exe --cli

Python, OpenCV, Cargo, FFmpeg and the Xvid C library are not required to run this build.
Windows and a working OpenGL graphics driver are required. The MSVC CRT is linked statically.
Edit config/ to retain your character settings; templates and manifests live under assets/.
The included assets/regions files are required. Keep the entire packaged assets tree.
Select your character profile and game window before starting an action.
If auto/window capture shows a black game body, select screen and keep the game visible.
GUI actions send live game input independently of the general CLI dry_run setting.
General CLI defaults: F8 pause/resume, F9 stop, Escape closes preview. GUI actions use F10.
The original extra-held-key stop limitation remains, and hotkeys still reach the game.
Summoner CLI defaults to dry; --live enables its input. Follow the required character/equipment starting conditions.
Recording uses direct Rust MPEG-4 in silent8fps AVI; it is not a full Xvid implementation.
Synthetic/source tests and a usable application do not establish full Summoner gameplay success.
See THIRD-PARTY-NOTICES.md and licenses/ for incorporated code/data and dependency notices.
'@ | Set-Content -LiteralPath (Join-Path $stage 'README.md') -Encoding utf8
    $manifest = [Collections.Generic.List[object]]::new()
    foreach ($file in Get-ChildItem -LiteralPath $stage -Recurse -File) {
        if ($file.Extension -in @('.py','.pyc','.pyd','.dll')) { throw "Unexpected runtime file: $($file.FullName)" }
        $relative = [IO.Path]::GetRelativePath($stage, $file.FullName).Replace('\', '/')
        $manifest.Add([ordered]@{ path=$relative; bytes=$file.Length; sha256=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant() })
    }
    # config/공용 assets/와 EXE는 원본과 바이트 지문까지 확인한다.
    foreach ($row in $manifest) {
        $sourceFile = if ($row.path -eq 'd2bot.exe') { $binary } elseif ($row.path.StartsWith('config/') -or $row.path.StartsWith('assets/')) { Join-Path $projectRoot $row.path } else { $null }
        if ($null -ne $sourceFile -and (Get-FileHash -LiteralPath $sourceFile -Algorithm SHA256).Hash.ToLowerInvariant() -cne $row.sha256) { throw "Package copy differs: $($row.path)" }
    }
    $data = [ordered]@{ owner='d2bot-windows-x64'; target=$target; crt='static'; python_runtime=$false; codec='direct-rust-mpeg4'; development_assets=[bool]$IncludeDevelopmentAssets; runtime_dlls=$imports; dependencies=$licenseIndex.Count; files=@($manifest.ToArray()) }
    ConvertTo-Json -InputObject $data -Depth 10 | Set-Content -LiteralPath (Join-Path $stage 'package-manifest.json') -Encoding utf8
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
    if (Test-Path -LiteralPath $destination) { throw 'Destination appeared during packaging; refusing overwrite.' }
    if (-not ([IO.Path]::GetFullPath($stage)).StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or -not $destination.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Move containment check failed.' }
    Move-Item -LiteralPath $stage -Destination $destination
    $published = $true
    $packageBytes = (Get-ChildItem -LiteralPath $destination -Recurse -File | Measure-Object -Property Length -Sum).Sum
    [ordered]@{ directory=$destination; files=$manifest.Count+1; bytes=$packageBytes; dependencies=$licenseIndex.Count } | ConvertTo-Json
}
finally {
    Pop-Location
    if (-not $published -and (Test-Path -LiteralPath $stage)) {
        $resolved = [IO.Path]::GetFullPath($stage)
        if ($resolved.StartsWith(([IO.Path]::GetFullPath($temporaryRoot).TrimEnd('\')+'\'), [StringComparison]::OrdinalIgnoreCase) -and [IO.Path]::GetFileName($resolved).StartsWith('package-stage-') -and -not ((Get-Item -LiteralPath $stage).Attributes -band [IO.FileAttributes]::ReparsePoint)) { Remove-Item -LiteralPath $stage -Recurse -Force }
    }
}
