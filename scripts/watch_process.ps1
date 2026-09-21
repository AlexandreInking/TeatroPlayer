# Vigila el consumo de un proceso mientras corre un spike.
#
#   powershell -ExecutionPolicy Bypass -File scripts/watch_process.ps1 `
#       -Name tp-spike -Seconds 300
#
# Mide CPU (como porcentaje de UN nucleo) y memoria de trabajo (RSS), y tambien
# handles e hilos: si el loop infinito de T-SPIKE-005 perdiera recursos, los
# handles o los hilos crecerian aunque la memoria se mantuviera estable.

param(
    [string]$Name = "tp-spike",
    [int]$Seconds = 300,
    [int]$Interval = 2
)

$cores = (Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors
$end = (Get-Date).AddSeconds($Seconds)
$samples = @()

Write-Host "vigilando '$Name' durante $Seconds s ($cores nucleos logicos)"

while ((Get-Date) -lt $end) {
    $p = Get-Process -Name $Name -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($p) {
        $samples += [PSCustomObject]@{
            t       = Get-Date
            cpu     = [double]$p.CPU
            ws      = [double]$p.WorkingSet64
            handles = [int]$p.HandleCount
            threads = [int]$p.Threads.Count
        }
    }
    Start-Sleep -Seconds $Interval
}

if ($samples.Count -lt 2) {
    Write-Host "no se pudo muestrear el proceso '$Name'"
    exit 1
}

$first = $samples[0]
$last = $samples[-1]
$wall = ($last.t - $first.t).TotalSeconds
$cpuDelta = $last.cpu - $first.cpu

$cpuPctOneCore = if ($wall -gt 0) { ($cpuDelta / $wall) * 100 } else { 0 }
$cpuPctAllCores = $cpuPctOneCore / $cores

$wsAll = $samples | ForEach-Object { $_.ws }
$wsMin = ($wsAll | Measure-Object -Minimum).Minimum / 1MB
$wsMax = ($wsAll | Measure-Object -Maximum).Maximum / 1MB
$wsFirst = $samples[0].ws / 1MB
$wsLast = $samples[-1].ws / 1MB

$hFirst = $samples[0].handles
$hLast = $samples[-1].handles
$tFirst = $samples[0].threads
$tLast = $samples[-1].threads

Write-Host ""
Write-Host "  muestras          : $($samples.Count) en $([math]::Round($wall, 1)) s"
Write-Host "  CPU (1 nucleo)    : $([math]::Round($cpuPctOneCore, 2)) %"
Write-Host "  CPU (todos)       : $([math]::Round($cpuPctAllCores, 2)) %"
Write-Host "  RSS inicial       : $([math]::Round($wsFirst, 1)) MB"
Write-Host "  RSS final         : $([math]::Round($wsLast, 1)) MB"
Write-Host "  RSS min / max     : $([math]::Round($wsMin, 1)) / $([math]::Round($wsMax, 1)) MB"
Write-Host "  RSS variacion     : $([math]::Round($wsMax - $wsMin, 1)) MB"
Write-Host "  handles inicio/fin: $hFirst / $hLast"
Write-Host "  hilos inicio/fin  : $tFirst / $tLast"

exit 0
