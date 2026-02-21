# vibereport installer for Windows — https://vibereport.dev
# Usage: irm https://vibereport.dev/install.ps1 | iex
$ErrorActionPreference = "Stop"

$repo = "vibereport/vibereport"
$binary = "vibereport"
$target = "x86_64-pc-windows-msvc"

# Fetch latest release tag
Write-Host "Fetching latest release..."
$release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest"
$tag = $release.tag_name
Write-Host "Latest release: $tag"

# Download
$url = "https://github.com/$repo/releases/download/$tag/$binary-$target.zip"
Write-Host "Downloading $binary-$target.zip..."
$tmp = New-TemporaryFile | Rename-Item -NewName { $_.Name + ".zip" } -PassThru
Invoke-WebRequest -Uri $url -OutFile $tmp.FullName

# Extract
$extractDir = Join-Path $env:TEMP "vibereport-install"
if (Test-Path $extractDir) { Remove-Item $extractDir -Recurse -Force }
Expand-Archive -Path $tmp.FullName -DestinationPath $extractDir
Remove-Item $tmp.FullName

# Install
$installDir = Join-Path $env:USERPROFILE ".vibereport\bin"
if (-not (Test-Path $installDir)) { New-Item -ItemType Directory -Path $installDir -Force | Out-Null }
Copy-Item (Join-Path $extractDir "$binary.exe") (Join-Path $installDir "$binary.exe") -Force
Remove-Item $extractDir -Recurse -Force

# Add to PATH if not present
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$installDir;$userPath", "User")
    Write-Host ""
    Write-Host "Added $installDir to your PATH. Restart your terminal to use it."
}

Write-Host ""
Write-Host "vibereport $tag installed to $installDir\$binary.exe"
Write-Host ""
Write-Host "Run 'vibereport' in any git repo to get started."
