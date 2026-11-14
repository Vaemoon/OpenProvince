@echo off
setlocal
cd /d "%~dp0"
if not exist "dist" mkdir "dist"
cargo build --locked --release --bin openprovince
if errorlevel 1 exit /b 1
copy /y "target\release\openprovince.exe" "dist\openprovince.exe" >nul
if errorlevel 1 exit /b 1
cargo build --locked --release --bin openprovince --no-default-features
if errorlevel 1 exit /b 1
copy /y "target\release\openprovince.exe" "dist\openprovince-archiver.exe" >nul
if errorlevel 1 exit /b 1
echo Built dist\openprovince.exe and dist\openprovince-archiver.exe
