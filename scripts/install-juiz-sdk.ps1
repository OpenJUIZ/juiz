# GitHub Releases から Windows 向けの libjuiz_sdk とヘッダ(juiz_sdk-<版>-x86_64-pc-windows-msvc.zip)を取得して展開する(issue #273)。
# C++ のプラグインを、juiz のソースを取得せずにビルドするために使う。Linux / macOS は install-juiz-sdk.sh を使う。
#
# 使い方: install-juiz-sdk.ps1 <版> [展開先]
#   版は juiz コマンドと同じもの(juiz --version、例: 0.0.27)。
#   展開先の既定は %USERPROFILE%\.juiz\sdk\<版>(環境変数 JUIZ_SDK_HOME で %USERPROFILE%\.juiz\sdk を変えられる)。
#
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/OpenJUIZ/juiz/main/scripts/install-juiz-sdk.ps1))) 0.0.27
#
# 対応: Windows x86_64(MSVC でビルドした DLL。MinGW など他のコンパイラのプラグインからも C ABI で使える)
# Windows PowerShell 5.1 と PowerShell 7 で動く。
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Version,
    [Parameter(Position = 1)][string]$Dest
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'  # Windows PowerShell 5.1 の Invoke-WebRequest は進捗表示でとても遅くなる

$Version = $Version.TrimStart('v')
if (-not $Dest) {
    $home_dir = if ($env:JUIZ_SDK_HOME) { $env:JUIZ_SDK_HOME } else { Join-Path $env:USERPROFILE '.juiz\sdk' }
    $Dest = Join-Path $home_dir $Version
}
$base_url = if ($env:JUIZ_SDK_BASE_URL) { $env:JUIZ_SDK_BASE_URL } else { 'https://github.com/OpenJUIZ/juiz/releases/download' }

$arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
if ($arch -ne 'AMD64') {
    Write-Error "$arch 向けの libjuiz_sdk は配布していません。juiz のリポジトリで cargo build -p juiz_sdk を実行してください。"
}
$target = 'x86_64-pc-windows-msvc'

$name = "juiz_sdk-$Version-$target"
$url = "$base_url/v$Version/$name.zip"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("juiz_sdk_" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    # Windows PowerShell 5.1 は既定で TLS 1.2 を使わないことがある
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    Write-Host "downloading $url"
    $zip = Join-Path $tmp "$name.zip"
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $zip
    } catch {
        Write-Error "$url を取得できません。版($Version)が公開されているか https://github.com/OpenJUIZ/juiz/releases を確認してください。"
    }
    Invoke-WebRequest -UseBasicParsing -Uri "$url.sha256" -OutFile "$zip.sha256"
    $expected = ((Get-Content "$zip.sha256" -Raw).Trim() -split '\s+')[0].ToLower()
    $actual = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
    if ($expected -ne $actual) {
        Write-Error "$name.zip のチェックサムが合いません"
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
@"
installed libjuiz_sdk $Version to $Dest

CMake: configure with -DCMAKE_PREFIX_PATH=$cmake_dest and write
  find_package(juiz_sdk $Version EXACT CONFIG REQUIRED)
  target_link_libraries(<plugin> PRIVATE juiz_sdk::juiz_sdk)
  juiz_sdk_copy_runtime(<plugin>)   # copies juiz_sdk.dll next to the plugin's DLL

Other build systems: add include directories $Dest\cjuiz\include and $Dest\cppjuiz\include,
  link $Dest\lib\juiz_sdk.dll.lib, and put $Dest\bin\juiz_sdk.dll next to the plugin's DLL
  (juiz looks for the DLLs a plugin depends on in the plugin's directory, and in PATH).
"@ | Write-Host
