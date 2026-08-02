@echo off
setlocal

fltmc >nul 2>&1
if errorlevel 1 (
  powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
  exit /b
)

echo Restoring UDP IPv4 checksum offload on the Intel I225-V adapter...
powershell -NoProfile -ExecutionPolicy Bypass -Command "$ErrorActionPreference='Stop'; $adapter=Get-NetAdapter | Where-Object InterfaceDescription -eq 'Intel(R) Ethernet Controller (3) I225-V' | Select-Object -First 1; if(-not $adapter){throw 'Intel I225-V adapter not found'}; Set-NetAdapterAdvancedProperty -Name $adapter.Name -RegistryKeyword '*UDPChecksumOffloadIPv4' -RegistryValue 3; Restart-NetAdapter -Name $adapter.Name; Get-NetAdapterAdvancedProperty -Name $adapter.Name -RegistryKeyword '*UDPChecksumOffloadIPv4' | Format-List DisplayName,DisplayValue,RegistryValue"
if errorlevel 1 (
  echo Restore failed. Open Device Manager and set UDP Checksum Offload IPv4 to Rx and Tx Enabled.
) else (
  echo Original setting restored.
)
pause
