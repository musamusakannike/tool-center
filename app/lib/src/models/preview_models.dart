import 'dart:convert';

class MetadataItemModel {
  final String label;
  final String value;

  const MetadataItemModel({required this.label, required this.value});

  factory MetadataItemModel.fromJson(Map<String, dynamic> json) {
    return MetadataItemModel(
      label: json['label']?.toString() ?? '',
      value: json['value']?.toString() ?? '',
    );
  }
}

class FileInspectionModel {
  final String path;
  final String fileName;
  final String extension;
  final String previewKind;
  final String? languageHint;
  final int sizeBytes;
  final String? modifiedIso;
  final String? createdIso;
  final bool readonly;
  final int? pageCount;
  final bool pdfEncrypted;
  final List<MetadataItemModel> metadata;

  const FileInspectionModel({
    required this.path,
    required this.fileName,
    required this.extension,
    required this.previewKind,
    this.languageHint,
    required this.sizeBytes,
    this.modifiedIso,
    this.createdIso,
    this.readonly = false,
    this.pageCount,
    this.pdfEncrypted = false,
    required this.metadata,
  });

  factory FileInspectionModel.fromJsonStr(String raw) {
    final json = jsonDecode(raw) as Map<String, dynamic>;
    return FileInspectionModel(
      path: json['path']?.toString() ?? '',
      fileName: json['file_name']?.toString() ?? '',
      extension: json['extension']?.toString() ?? '',
      previewKind: json['preview_kind']?.toString() ?? 'binary',
      languageHint: json['language_hint']?.toString(),
      sizeBytes: (json['size_bytes'] as num?)?.toInt() ?? 0,
      modifiedIso: json['modified_iso']?.toString(),
      createdIso: json['created_iso']?.toString(),
      readonly: json['readonly'] as bool? ?? false,
      pageCount: (json['page_count'] as num?)?.toInt(),
      pdfEncrypted: json['pdf_encrypted'] as bool? ?? false,
      metadata: (json['metadata'] as List<dynamic>? ?? [])
          .whereType<Map<String, dynamic>>()
          .map(MetadataItemModel.fromJson)
          .toList(),
    );
  }
}

class TextPreviewModel {
  final String text;
  final bool truncated;
  final int totalBytes;
  final int lineCountEstimate;
  final List<String>? csvHeaders;
  final List<List<String>>? csvRows;

  const TextPreviewModel({
    required this.text,
    required this.truncated,
    required this.totalBytes,
    required this.lineCountEstimate,
    this.csvHeaders,
    this.csvRows,
  });

  factory TextPreviewModel.fromJsonStr(String raw) {
    final json = jsonDecode(raw) as Map<String, dynamic>;
    return TextPreviewModel(
      text: json['text']?.toString() ?? '',
      truncated: json['truncated'] as bool? ?? false,
      totalBytes: (json['total_bytes'] as num?)?.toInt() ?? 0,
      lineCountEstimate: (json['line_count_estimate'] as num?)?.toInt() ?? 0,
      csvHeaders: (json['csv_headers'] as List<dynamic>?)
          ?.map((e) => e.toString())
          .toList(),
      csvRows: (json['csv_rows'] as List<dynamic>?)
          ?.whereType<List<dynamic>>()
          .map((row) => row.map((c) => c.toString()).toList())
          .toList(),
    );
  }
}
