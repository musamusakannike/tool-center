import 'dart:convert';

enum ToolCategoryId {
  images,
  video,
  audio,
  pdf,
  files,
  convert;

  static ToolCategoryId fromString(String raw) {
    switch (raw.toLowerCase()) {
      case 'images':
        return ToolCategoryId.images;
      case 'video':
        return ToolCategoryId.video;
      case 'audio':
        return ToolCategoryId.audio;
      case 'pdf':
        return ToolCategoryId.pdf;
      case 'convert':
        return ToolCategoryId.convert;
      case 'files':
      default:
        return ToolCategoryId.files;
    }
  }
}

class ToolInputSpecModel {
  final List<String> acceptedExtensions;
  final int minFiles;
  final int? maxFiles;
  final bool allowDirectories;

  const ToolInputSpecModel({
    required this.acceptedExtensions,
    required this.minFiles,
    required this.maxFiles,
    required this.allowDirectories,
  });

  factory ToolInputSpecModel.fromJson(Map<String, dynamic> json) {
    return ToolInputSpecModel(
      acceptedExtensions: (json['accepted_extensions'] as List<dynamic>? ?? [])
          .map((e) => e.toString())
          .toList(),
      minFiles: (json['min_files'] as num?)?.toInt() ?? 1,
      maxFiles: (json['max_files'] as num?)?.toInt(),
      allowDirectories: json['allow_directories'] as bool? ?? false,
    );
  }
}

class SelectChoiceModel {
  final String value;
  final String label;
  final String? description;

  const SelectChoiceModel({
    required this.value,
    required this.label,
    this.description,
  });

  factory SelectChoiceModel.fromJson(Map<String, dynamic> json) {
    return SelectChoiceModel(
      value: json['value']?.toString() ?? '',
      label: json['label']?.toString() ?? '',
      description: json['description']?.toString(),
    );
  }
}

class VisibilityConditionModel {
  final String optionKey;
  final dynamic equalsValue;

  const VisibilityConditionModel({
    required this.optionKey,
    required this.equalsValue,
  });

  factory VisibilityConditionModel.fromJson(Map<String, dynamic> json) {
    return VisibilityConditionModel(
      optionKey: json['option_key']?.toString() ?? '',
      equalsValue: json['equals_value'],
    );
  }

  bool matches(Map<String, dynamic> currentValues) {
    return currentValues[optionKey] == equalsValue;
  }
}

enum OptionKindType {
  boolean,
  integer,
  number,
  text,
  select,
  positionGrid,
  filePath,
}

class OptionFieldModel {
  final String key;
  final String label;
  final String labelKey;
  final String hint;
  final String hintKey;
  final OptionKindType kindType;
  final num? min;
  final num? max;
  final num? step;
  final String? unit;
  final String? placeholder;
  final bool secret;
  final List<SelectChoiceModel> choices;
  final List<String> acceptedExtensions;
  final dynamic defaultValue;
  final bool requiredField;
  final VisibilityConditionModel? visibleWhen;

  const OptionFieldModel({
    required this.key,
    required this.label,
    required this.labelKey,
    required this.hint,
    required this.hintKey,
    required this.kindType,
    this.min,
    this.max,
    this.step,
    this.unit,
    this.placeholder,
    this.secret = false,
    this.choices = const [],
    this.acceptedExtensions = const [],
    this.defaultValue,
    this.requiredField = false,
    this.visibleWhen,
  });

  bool isVisible(Map<String, dynamic> currentValues) {
    final cond = visibleWhen;
    if (cond == null) return true;
    return cond.matches(currentValues);
  }

  factory OptionFieldModel.fromJson(Map<String, dynamic> json) {
    final kind = json['kind'] as Map<String, dynamic>? ?? const {};
    final rawType = kind['type']?.toString() ?? 'text';
    OptionKindType parsedType;
    switch (rawType) {
      case 'boolean':
        parsedType = OptionKindType.boolean;
      case 'integer':
        parsedType = OptionKindType.integer;
      case 'number':
        parsedType = OptionKindType.number;
      case 'select':
        parsedType = OptionKindType.select;
      case 'position_grid':
        parsedType = OptionKindType.positionGrid;
      case 'file_path':
        parsedType = OptionKindType.filePath;
      case 'text':
      default:
        parsedType = OptionKindType.text;
    }

    return OptionFieldModel(
      key: json['key']?.toString() ?? '',
      label: json['label']?.toString() ?? '',
      labelKey: json['label_key']?.toString() ?? '',
      hint: json['hint']?.toString() ?? '',
      hintKey: json['hint_key']?.toString() ?? '',
      kindType: parsedType,
      min: kind['min'] as num?,
      max: kind['max'] as num?,
      step: kind['step'] as num?,
      unit: kind['unit']?.toString(),
      placeholder: kind['placeholder']?.toString(),
      secret: kind['secret'] as bool? ?? false,
      choices: (kind['choices'] as List<dynamic>? ?? [])
          .whereType<Map<String, dynamic>>()
          .map(SelectChoiceModel.fromJson)
          .toList(),
      acceptedExtensions: (kind['accepted_extensions'] as List<dynamic>? ?? [])
          .map((e) => e.toString())
          .toList(),
      defaultValue: json['default_value'],
      requiredField: json['required'] as bool? ?? false,
      visibleWhen: json['visible_when'] is Map<String, dynamic>
          ? VisibilityConditionModel.fromJson(
              json['visible_when'] as Map<String, dynamic>,
            )
          : null,
    );
  }
}

class ToolDescriptorModel {
  final String id;
  final String name;
  final String nameKey;
  final String description;
  final String descriptionKey;
  final ToolCategoryId category;
  final String icon;
  final List<String> keywords;
  final ToolInputSpecModel inputSpec;
  final List<OptionFieldModel> optionsSchema;
  final String defaultSuffix;

  const ToolDescriptorModel({
    required this.id,
    required this.name,
    required this.nameKey,
    required this.description,
    required this.descriptionKey,
    required this.category,
    required this.icon,
    required this.keywords,
    required this.inputSpec,
    required this.optionsSchema,
    required this.defaultSuffix,
  });

  Map<String, dynamic> defaultOptionsMap() {
    final map = <String, dynamic>{};
    for (final field in optionsSchema) {
      map[field.key] = field.defaultValue;
    }
    return map;
  }

  bool matchesQuery(String query) {
    final q = query.trim().toLowerCase();
    if (q.isEmpty) return true;
    if (name.toLowerCase().contains(q)) return true;
    if (description.toLowerCase().contains(q)) return true;
    if (category.name.toLowerCase().contains(q)) return true;
    if (keywords.any((k) => k.toLowerCase().contains(q))) return true;
    if (inputSpec.acceptedExtensions.any((e) => e.toLowerCase().contains(q))) {
      return true;
    }
    return false;
  }

  factory ToolDescriptorModel.fromJson(Map<String, dynamic> json) {
    return ToolDescriptorModel(
      id: json['id']?.toString() ?? '',
      name: json['name']?.toString() ?? '',
      nameKey: json['name_key']?.toString() ?? '',
      description: json['description']?.toString() ?? '',
      descriptionKey: json['description_key']?.toString() ?? '',
      category: ToolCategoryId.fromString(json['category']?.toString() ?? ''),
      icon: json['icon']?.toString() ?? 'build',
      keywords: (json['keywords'] as List<dynamic>? ?? [])
          .map((e) => e.toString())
          .toList(),
      inputSpec: ToolInputSpecModel.fromJson(
        json['input_spec'] as Map<String, dynamic>? ?? const {},
      ),
      optionsSchema: (json['options_schema'] as List<dynamic>? ?? [])
          .whereType<Map<String, dynamic>>()
          .map(OptionFieldModel.fromJson)
          .toList(),
      defaultSuffix: json['default_suffix']?.toString() ?? '_out',
    );
  }

  static List<ToolDescriptorModel> decodeList(String rawJson) {
    final decoded = jsonDecode(rawJson) as List<dynamic>;
    return decoded
        .whereType<Map<String, dynamic>>()
        .map(ToolDescriptorModel.fromJson)
        .toList();
  }
}

class OutputArtifactModel {
  final String path;
  final String label;
  final int sizeBytes;
  final int? originalSizeBytes;

  const OutputArtifactModel({
    required this.path,
    required this.label,
    required this.sizeBytes,
    this.originalSizeBytes,
  });

  factory OutputArtifactModel.fromJson(Map<String, dynamic> json) {
    return OutputArtifactModel(
      path: json['path']?.toString() ?? '',
      label: json['label']?.toString() ?? '',
      sizeBytes: (json['size_bytes'] as num?)?.toInt() ?? 0,
      originalSizeBytes: (json['original_size_bytes'] as num?)?.toInt(),
    );
  }
}

enum JobStatusId {
  queued,
  running,
  completed,
  cancelled,
  failed;

  bool get isActive => this == JobStatusId.queued || this == JobStatusId.running;

  static JobStatusId fromString(String raw) {
    switch (raw.toLowerCase()) {
      case 'queued':
        return JobStatusId.queued;
      case 'running':
        return JobStatusId.running;
      case 'completed':
        return JobStatusId.completed;
      case 'cancelled':
        return JobStatusId.cancelled;
      case 'failed':
      default:
        return JobStatusId.failed;
    }
  }
}

class JobSnapshotModel {
  final String jobId;
  final String toolId;
  final String toolName;
  final JobStatusId status;
  final double progress;
  final String stepMessage;
  final int inputCount;
  final List<OutputArtifactModel> outputs;
  final String? summary;
  final List<String> warnings;
  final String? reportJson;
  final String? errorMessage;
  final String? errorDetails;
  final int createdAtMs;
  final int? durationMs;

  const JobSnapshotModel({
    required this.jobId,
    required this.toolId,
    required this.toolName,
    required this.status,
    required this.progress,
    required this.stepMessage,
    required this.inputCount,
    required this.outputs,
    this.summary,
    this.warnings = const [],
    this.reportJson,
    this.errorMessage,
    this.errorDetails,
    required this.createdAtMs,
    this.durationMs,
  });

  factory JobSnapshotModel.fromJson(Map<String, dynamic> json) {
    return JobSnapshotModel(
      jobId: json['job_id']?.toString() ?? '',
      toolId: json['tool_id']?.toString() ?? '',
      toolName: json['tool_name']?.toString() ?? '',
      status: JobStatusId.fromString(json['status']?.toString() ?? 'failed'),
      progress: (json['progress'] as num?)?.toDouble() ?? 0.0,
      stepMessage: json['step_message']?.toString() ?? '',
      inputCount: (json['input_count'] as num?)?.toInt() ?? 0,
      outputs: (json['outputs'] as List<dynamic>? ?? [])
          .whereType<Map<String, dynamic>>()
          .map(OutputArtifactModel.fromJson)
          .toList(),
      summary: json['summary']?.toString(),
      warnings: (json['warnings'] as List<dynamic>? ?? [])
          .map((e) => e.toString())
          .toList(),
      reportJson: json['report_json']?.toString(),
      errorMessage: json['error_message']?.toString(),
      errorDetails: json['error_details']?.toString(),
      createdAtMs: (json['created_at_ms'] as num?)?.toInt() ?? 0,
      durationMs: (json['duration_ms'] as num?)?.toInt(),
    );
  }

  static List<JobSnapshotModel> decodeList(String rawJson) {
    final decoded = jsonDecode(rawJson) as List<dynamic>;
    return decoded
        .whereType<Map<String, dynamic>>()
        .map(JobSnapshotModel.fromJson)
        .toList();
  }
}

class AiModelStatusModel {
  final String id;
  final String name;
  final String capability;
  final String spdxLicense;
  final String description;
  final int sizeBytes;
  final bool installed;
  final bool v2Planned;

  const AiModelStatusModel({
    required this.id,
    required this.name,
    required this.capability,
    required this.spdxLicense,
    required this.description,
    required this.sizeBytes,
    required this.installed,
    required this.v2Planned,
  });

  factory AiModelStatusModel.fromJson(Map<String, dynamic> json) {
    final spec = json['spec'] as Map<String, dynamic>? ?? const {};
    return AiModelStatusModel(
      id: spec['id']?.toString() ?? '',
      name: spec['name']?.toString() ?? '',
      capability: spec['capability']?.toString() ?? '',
      spdxLicense: spec['spdx_license']?.toString() ?? '',
      description: spec['description']?.toString() ?? '',
      sizeBytes: (spec['size_bytes'] as num?)?.toInt() ?? 0,
      installed: json['installed'] as bool? ?? false,
      v2Planned: json['v2_planned'] as bool? ?? true,
    );
  }
}

class FlaggedExcludedModelInfo {
  final String name;
  final String upstreamLicense;
  final String reason;

  const FlaggedExcludedModelInfo({
    required this.name,
    required this.upstreamLicense,
    required this.reason,
  });

  factory FlaggedExcludedModelInfo.fromJson(Map<String, dynamic> json) {
    return FlaggedExcludedModelInfo(
      name: json['name']?.toString() ?? '',
      upstreamLicense: json['upstream_license']?.toString() ?? '',
      reason: json['reason']?.toString() ?? '',
    );
  }
}

String formatByteSize(int bytes) {
  if (bytes < 1024) return '$bytes B';
  final kb = bytes / 1024.0;
  if (kb < 1024) return '${kb.toStringAsFixed(1)} KB';
  final mb = kb / 1024.0;
  if (mb < 1024) return '${mb.toStringAsFixed(1)} MB';
  final gb = mb / 1024.0;
  return '${gb.toStringAsFixed(2)} GB';
}
