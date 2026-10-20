// Windows PDF page rendering through Windows.Data.Pdf, driven from Windows
// PowerShell. Fail-closed: when powershell.exe is missing, or the WinRT call
// fails, the page access throws OcrDocumentFailure and OCR reports the page as
// unreadable. UNVERIFIED: this has not run on a Windows host. There is no
// searchable-text API in Windows.Data.Pdf, so pageText returns empty.
library;

import 'dart:io';

import 'operations/process_scans.dart' show OcrDocumentFailure;
import 'ocr.dart';

/// PowerShell script: loads the PDF with Windows.Data.Pdf, renders one page to
/// an in-memory stream, decodes it to PNG with System.Drawing.
const String windowsPdfScript = r'''
param([string]$PdfPath, [int]$PageIndex, [int]$Dpi, [string]$Output)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Runtime.WindowsRuntime
Add-Type -AssemblyName System.Drawing
[void][Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]
[void][Windows.Data.Pdf.PdfDocument, Windows.Data.Pdf, ContentType = WindowsRuntime]
[void][Windows.Storage.Streams.InMemoryRandomAccessStream, Windows.Storage.Streams, ContentType = WindowsRuntime]
$asTask = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
  $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and
  $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' } | Select-Object -First 1
function Await-Op($operation, [Type]$resultType) {
  $task = $asTask.MakeGenericMethod($resultType).Invoke($null, @($operation))
  $task.Wait(-1) | Out-Null
  return $task.Result
}
$file = Await-Op ([Windows.Storage.StorageFile]::GetFileFromPathAsync($PdfPath)) ([Windows.Storage.StorageFile])
$document = Await-Op ([Windows.Data.Pdf.PdfDocument]::LoadFromFileAsync($file)) ([Windows.Data.Pdf.PdfDocument])
if ($PageIndex -lt 0 -or $PageIndex -ge $document.PageCount) { throw 'page index is out of range' }
$page = $document.GetPage([uint32]$PageIndex)
$scale = $Dpi / 72.0
$options = New-Object Windows.Data.Pdf.PdfPageRenderOptions
$options.DestinationWidth = [uint32][math]::Round($page.Size.Width * $scale)
$options.DestinationHeight = [uint32][math]::Round($page.Size.Height * $scale)
$stream = New-Object Windows.Storage.Streams.InMemoryRandomAccessStream
$action = $page.RenderToStreamAsync($stream, $options)
$actionTask = [System.WindowsRuntimeSystemExtensions]::AsTask($action)
$actionTask.Wait(-1) | Out-Null
$stream.Seek(0) | Out-Null
$bytes = New-Object byte[] $stream.Size
$inputStream = $stream.GetInputStreamAt(0)
$dataReader = New-Object Windows.Storage.Streams.DataReader($inputStream)
Await-Op ($dataReader.LoadAsync([uint32]$stream.Size)) ([uint32]) | Out-Null
$dataReader.ReadBytes($bytes)
$image = [System.Drawing.Image]::FromStream((New-Object System.IO.MemoryStream(,$bytes)))
$image.Save($Output, [System.Drawing.Imaging.ImageFormat]::Png)
$image.Dispose()
''';

final class WindowsPdfPageRenderer implements PdfPageRenderer {
  const WindowsPdfPageRenderer();

  /// True only on Windows hosts with PowerShell on PATH.
  static bool get available => Platform.isWindows;

  @override
  Future<PdfPageSession> openSession(String path) async {
    if (!available) {
      throw const OcrDocumentFailure('Windows PDF engine is unavailable');
    }
    return _WindowsPdfSession(path);
  }
}

final class _WindowsPdfSession implements PdfPageSession {
  _WindowsPdfSession(this._input);

  final String _input;

  @override
  Future<String> pageText(int pageIndex) async => '';

  @override
  Future<void> renderPagePng(String output, int dpi, int pageIndex) async {
    final dir = Directory.systemTemp.createTempSync('docflow-winpdf-');
    try {
      final script = File('${dir.path}/render.ps1')
        ..writeAsStringSync(windowsPdfScript);
      final result = await Process.run('powershell.exe', [
        '-NoProfile',
        '-NonInteractive',
        '-ExecutionPolicy',
        'Bypass',
        '-File',
        script.path,
        '-PdfPath',
        _input,
        '-PageIndex',
        '$pageIndex',
        '-Dpi',
        '$dpi',
        '-Output',
        output,
      ]).timeout(const Duration(seconds: 90));
      if (result.exitCode != 0) {
        throw OcrDocumentFailure('Windows PDF render failed (exit ${result.exitCode})');
      }
    } finally {
      dir.deleteSync(recursive: true);
    }
  }

  @override
  Future<void> close() async {}
}
