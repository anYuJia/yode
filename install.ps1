param(
    [string]$Version = $env:VERSION,
    [string]$InstallDir = $(if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { Join-Path $HOME ".local\bin" })
)

$ErrorActionPreference = "Stop"
$Repo = "anYuJia/yode"

function Info($Message) {
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Fail($Message) {
    Write-Error $Message
    exit 1
}

function Resolve-Version {
    if ($Version) {
        return $Version
    }
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    if (-not $release.tag_name) {
        Fail "Could not determine latest release. Set VERSION explicitly."
    }
    return $release.tag_name
}

function Verify-Checksum($Version, $Archive, $ArchivePath, $ChecksumPath) {
    $checksumUrl = "https://github.com/$Repo/releases/download/$Version/SHA256SUMS"
    Info "Checksum: $checksumUrl"
    Invoke-WebRequest -Uri $checksumUrl -OutFile $ChecksumPath

    $line = Get-Content $ChecksumPath | Where-Object {
        $parts = $_ -split '\s+'
        $parts.Length -ge 2 -and ($parts[1] -eq $Archive -or $parts[1] -eq "*$Archive")
    } | Select-Object -First 1
    if (-not $line) {
        Fail "Checksum file does not contain $Archive."
    }

    $expected = (($line -split '\s+')[0]).ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 $ArchivePath).Hash.ToLowerInvariant()
    if ($expected -ne $actual) {
        Fail "Checksum verification failed for $Archive."
    }
    Info "Checksum verified."
}

$version = Resolve-Version
$archive = "yode-x86_64-pc-windows-msvc.zip"
$url = "https://github.com/$Repo/releases/download/$version/$archive"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("yode-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null

try {
    Info "Installing yode $version (Windows x86_64)"
    Info "Download: $url"
    $archivePath = Join-Path $tmp $archive
    Invoke-WebRequest -Uri $url -OutFile $archivePath
    Verify-Checksum $version $archive $archivePath (Join-Path $tmp "SHA256SUMS")

    Expand-Archive -Path $archivePath -DestinationPath $tmp -Force
    $binary = Get-ChildItem -Path $tmp -Filter "yode.exe" -Recurse | Select-Object -First 1
    if (-not $binary) {
        Fail "Could not find yode.exe in archive."
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path $binary.FullName -Destination (Join-Path $InstallDir "yode.exe") -Force
    Info "Installed to $(Join-Path $InstallDir "yode.exe")"

    & (Join-Path $InstallDir "yode.exe") --version | Out-Null
    Info "Verification passed."
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
