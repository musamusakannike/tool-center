import 'package:file_picker/file_picker.dart';
import 'package:material_ui/material_ui.dart';

import '../../models/tool_models.dart';

/// Automatically renders plain-language controls for any `Tool`'s `options_schema`.
class SchemaOptionsForm extends StatefulWidget {
  final List<OptionFieldModel> schema;
  final Map<String, dynamic> values;
  final ValueChanged<Map<String, dynamic>> onChanged;

  const SchemaOptionsForm({
    super.key,
    required this.schema,
    required this.values,
    required this.onChanged,
  });

  @override
  State<SchemaOptionsForm> createState() => _SchemaOptionsFormState();
}

class _SchemaOptionsFormState extends State<SchemaOptionsForm> {
  final Map<String, bool> _obscureMap = {};

  void _updateValue(String key, dynamic newValue) {
    final next = Map<String, dynamic>.from(widget.values);
    next[key] = newValue;
    widget.onChanged(next);
  }

  @override
  Widget build(BuildContext context) {
    final visibleFields = widget.schema
        .where((f) => f.isVisible(widget.values))
        .toList();

    if (visibleFields.isEmpty) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 8),
        child: Text(
          'Ready to run with automatic settings — no extra configuration needed.',
          style: Theme.of(context).textTheme.bodyMedium?.copyWith(
            color: Theme.of(context).colorScheme.onSurfaceVariant,
          ),
        ),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (int i = 0; i < visibleFields.length; i++) ...[
          if (i > 0) const SizedBox(height: 16),
          _buildField(context, visibleFields[i]),
        ],
      ],
    );
  }

  Widget _buildField(BuildContext context, OptionFieldModel field) {
    final current = widget.values.containsKey(field.key)
        ? widget.values[field.key]
        : field.defaultValue;

    switch (field.kindType) {
      case OptionKindType.boolean:
        final checked = current is bool ? current : false;
        return SwitchListTile(
          contentPadding: EdgeInsets.zero,
          title: Text(
            field.label,
            style: Theme.of(
              context,
            ).textTheme.titleSmall?.copyWith(fontWeight: FontWeight.w600),
          ),
          subtitle: Text(field.hint),
          value: checked,
          onChanged: (val) => _updateValue(field.key, val),
        );

      case OptionKindType.select:
        final selected =
            current?.toString() ??
            (field.choices.isNotEmpty ? field.choices.first.value : '');
        return _FieldWrapper(
          label: field.label,
          hint: field.hint,
          child: DropdownButtonFormField<String>(
            initialValue: field.choices.any((c) => c.value == selected)
                ? selected
                : (field.choices.isNotEmpty ? field.choices.first.value : null),
            items: field.choices
                .map(
                  (choice) => DropdownMenuItem<String>(
                    value: choice.value,
                    child: Text(
                      choice.description != null
                          ? '${choice.label} — ${choice.description}'
                          : choice.label,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                )
                .toList(),
            onChanged: (val) {
              if (val != null) _updateValue(field.key, val);
            },
          ),
        );

      case OptionKindType.integer:
      case OptionKindType.number:
        return _buildNumericField(context, field, current);

      case OptionKindType.positionGrid:
        final selectedPos = current?.toString() ?? 'bottom_right';
        return _FieldWrapper(
          label: field.label,
          hint: field.hint,
          child: _PositionGridPicker(
            selected: selectedPos,
            onSelected: (pos) => _updateValue(field.key, pos),
          ),
        );

      case OptionKindType.filePath:
        final pathStr = current?.toString() ?? '';
        return _FieldWrapper(
          label: field.label,
          hint: field.hint,
          child: Row(
            children: [
              Expanded(
                child: TextFormField(
                  key: ValueKey('${field.key}:$pathStr'),
                  initialValue: pathStr,
                  decoration: const InputDecoration(
                    hintText: 'Choose a file...',
                  ),
                  onChanged: (v) => _updateValue(field.key, v),
                ),
              ),
              const SizedBox(width: 8),
              OutlinedButton.icon(
                onPressed: () async {
                  final res = await FilePicker.pickFile(
                    type: field.acceptedExtensions.isEmpty
                        ? FileType.any
                        : FileType.custom,
                    allowedExtensions: field.acceptedExtensions.isEmpty
                        ? null
                        : field.acceptedExtensions,
                  );
                  final picked = res?.path;
                  if (picked != null && picked.isNotEmpty) {
                    _updateValue(field.key, picked);
                  }
                },
                icon: const Icon(Icons.folder_open_rounded, size: 18),
                label: const Text('Browse'),
              ),
            ],
          ),
        );

      case OptionKindType.text:
        final isSecret = field.secret;
        final obscured = _obscureMap[field.key] ?? isSecret;
        return _FieldWrapper(
          label: field.label,
          hint: field.hint,
          child: TextFormField(
            initialValue: current?.toString() ?? '',
            obscureText: obscured,
            decoration: InputDecoration(
              hintText: field.placeholder,
              suffixIcon: isSecret
                  ? IconButton(
                      tooltip: obscured ? 'Show password' : 'Hide password',
                      icon: Icon(
                        obscured
                            ? Icons.visibility_outlined
                            : Icons.visibility_off_outlined,
                        size: 18,
                      ),
                      onPressed: () {
                        setState(() {
                          _obscureMap[field.key] = !obscured;
                        });
                      },
                    )
                  : null,
            ),
            onChanged: (v) => _updateValue(field.key, v),
          ),
        );
    }
  }

  Widget _buildNumericField(
    BuildContext context,
    OptionFieldModel field,
    dynamic current,
  ) {
    final isInt = field.kindType == OptionKindType.integer;
    final numVal = (current is num)
        ? current.toDouble()
        : double.tryParse(current?.toString() ?? '') ??
              (field.min?.toDouble() ?? 0.0);

    final min = field.min?.toDouble();
    final max = field.max?.toDouble();
    final unitSuffix = field.unit != null ? ' ${field.unit}' : '';

    if (min != null && max != null && max > min && (max - min) <= 400) {
      final clamped = numVal.clamp(min, max);
      final display = isInt
          ? '${clamped.round()}$unitSuffix'
          : '${clamped.toStringAsFixed(2)}$unitSuffix';
      return _FieldWrapper(
        label: '${field.label} ($display)',
        hint: field.hint,
        child: Slider(
          min: min,
          max: max,
          divisions: isInt ? (max - min).round().clamp(1, 200) : 100,
          value: clamped,
          onChanged: (v) {
            _updateValue(field.key, isInt ? v.round() : v);
          },
        ),
      );
    }

    return _FieldWrapper(
      label: field.label,
      hint: field.hint,
      child: TextFormField(
        initialValue: isInt ? numVal.round().toString() : numVal.toString(),
        keyboardType: TextInputType.number,
        decoration: InputDecoration(suffixText: field.unit),
        onChanged: (raw) {
          if (isInt) {
            final parsed = int.tryParse(raw.trim());
            if (parsed != null) _updateValue(field.key, parsed);
          } else {
            final parsed = double.tryParse(raw.trim());
            if (parsed != null) _updateValue(field.key, parsed);
          }
        },
      ),
    );
  }
}

class _FieldWrapper extends StatelessWidget {
  final String label;
  final String hint;
  final Widget child;

  const _FieldWrapper({
    required this.label,
    required this.hint,
    required this.child,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: theme.textTheme.titleSmall?.copyWith(
            fontWeight: FontWeight.w600,
          ),
        ),
        if (hint.isNotEmpty) ...[
          const SizedBox(height: 2),
          Text(
            hint,
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ],
        const SizedBox(height: 8),
        child,
      ],
    );
  }
}

class _PositionGridPicker extends StatelessWidget {
  static const _positions = <(String, String)>[
    ('top_left', 'Top Left'),
    ('top_center', 'Top'),
    ('top_right', 'Top Right'),
    ('center_left', 'Left'),
    ('center', 'Center'),
    ('center_right', 'Right'),
    ('bottom_left', 'Bottom Left'),
    ('bottom_center', 'Bottom'),
    ('bottom_right', 'Bottom Right'),
  ];

  final String selected;
  final ValueChanged<String> onSelected;

  const _PositionGridPicker({
    required this.selected,
    required this.onSelected,
  });

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Wrap(
      spacing: 8,
      runSpacing: 8,
      children: _positions.map((entry) {
        final isSelected = entry.$1 == selected;
        return ChoiceChip(
          label: Text(entry.$2),
          selected: isSelected,
          selectedColor: scheme.primaryContainer,
          onSelected: (_) => onSelected(entry.$1),
        );
      }).toList(),
    );
  }
}
