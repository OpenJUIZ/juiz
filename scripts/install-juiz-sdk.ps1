# Download libjuiz_sdk and its headers for Windows (juiz_sdk-<version>-x86_64-pc-windows-msvc.zip)
# from GitHub Releases and extract them (issue #273). Use install-juiz-sdk.sh on Linux / macOS.
# This lets you build C++ plugins without fetching and building the juiz sources.
#
# Usage: install-juiz-sdk.ps1 <version> [dest]
#   version: the same version as the juiz command (juiz --version), e.g. 0.0.27
#   dest:    defaults to %USERPROFILE%\.juiz\sdk\<version>
#            (set JUIZ_SDK_HOME to change %USERPROFILE%\.juiz\sdk)
#
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/OpenJUIZ/juiz/main/scripts/install-juiz-sdk.ps1))) 0.0.27
#
# Supported: Windows x86_64 (a DLL built with MSVC; plugins built with MinGW etc. can use it through the C ABI).
# Works with Windows PowerShell 5.1 and PowerShell 7.
#
# Keep this file ASCII only: Windows PowerShell 5.1 reads a UTF-8 file without BOM in the ANSI code page,
# so Japanese text breaks the parser.
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Version,
    [Parameter(Position = 1)][string]$Dest
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'  # the progress bar makes Invoke-WebRequest very slow on PowerShell 5.1

$Version = $Version.TrimStart('v')
if (-not $Dest) {
    $home_dir = if ($env:JUIZ_SDK_HOME) { $env:JUIZ_SDK_HOME } else { Join-Path $env:USERPROFILE '.juiz\sdk' }
    $Dest = Join-Path $home_dir $Version
}
$base_url = if ($env:JUIZ_SDK_BASE_URL) { $env:JUIZ_SDK_BASE_URL } else { 'https://github.com/OpenJUIZ/juiz/releases/download' }

$arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
if ($arch -ne 'AMD64') {
    Write-Error "libjuiz_sdk is not distributed for $arch. Build it with 'cargo build -p juiz_sdk' in the juiz repository."
}
$target = 'x86_64-pc-windows-msvc'

$name = "juiz_sdk-$Version-$target"
$url = "$base_url/v$Version/$name.zip"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("juiz_sdk_" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    # Windows PowerShell 5.1 may not use TLS 1.2 by default
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    Write-Host "downloading $url"
    $zip = Join-Path $tmp "$name.zip"
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $zip
    } catch {
        Write-Error "Cannot download $url. Check that version $Version is published at https://github.com/OpenJUIZ/juiz/releases"
    }
    Invoke-WebRequest -UseBasicParsing -Uri "$url.sha256" -OutFile "$zip.sha256"
    $expected = ((Get-Content "$zip.sha256" -Raw).Trim() -split '\s+')[0].ToLower()
    $actual = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
    if ($expected -ne $actual) {
        Write-Error "Checksum mismatch: $name.zip"
    }

    Expand-Archive -Path $zip -DestinationPath $tmp
    $parent = Split-Path -Parent $Dest
    if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
    if (Test-Path $Dest) { Remove-Item -Recurse -Force $Dest }
    Move-Item (Join-Path $tmp $name) $Dest
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

$cmake_dest = $Dest -replace '\\', '/'
Write-Host @"
installed libjuiz_sdk $Version to $Dest

CMake: configure with -DCMAKE_PREFIX_PATH=$cmake_dest and write
  find_package(juiz_sdk $Version EXACT CONFIG REQUIRED)
  target_link_libraries(<plugin> PRIVATE juiz_sdk::juiz_sdk)
  juiz_sdk_copy_runtime(<plugin>)   # copies juiz_sdk.dll next to the plugin's DLL

Other build systems: add include directories $Dest\cjuiz\include and $Dest\cppjuiz\include,
  link $Dest\lib\juiz_sdk.dll.lib, and put $Dest\bin\juiz_sdk.dll next to the plugin's DLL
  (juiz looks for the DLLs a plugin depends on in the plugin's directory, and in PATH).
"@
