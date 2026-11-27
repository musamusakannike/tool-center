use core_image::ImageConvertTool;
use core_jobs::{
    Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError, ToolOptions, ToolRegistry,
    ToolRunResult,
};
use core_media::{AudioConvertTool, VideoConvertTool};
use core_pdf::{ImagesToPdfTool, PdfToImagesTool};
use std::path::PathBuf;

pub fn register_conversion_tools(registry: &ToolRegistry) {
    registry.register(UnifiedImageConvertTool);
    registry.register(UnifiedAudioConvertTool);
    registry.register(UnifiedVideoConvertTool);
    registry.register(UnifiedPdfToImagesTool);
    registry.register(UnifiedImagesToPdfTool);
}

pub struct UnifiedImageConvertTool;

impl Tool for UnifiedImageConvertTool {
    fn descriptor(&self) -> ToolDescriptor {
        let mut base = ImageConvertTool.descriptor();
        base.id = "convert.image".into();
        base.name = "Image ⇄ Image".into();
        base.name_key = "tool_conv_image".into();
        base.category = ToolCategory::Convert;
        base.icon = "swap".into();
        base
    }

    fn validate(&self, inputs: &[PathBuf], options: &ToolOptions) -> Result<(), ToolError> {
        ImageConvertTool.validate(inputs, options)
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        ImageConvertTool.run(ctx)
    }
}

pub struct UnifiedAudioConvertTool;

impl Tool for UnifiedAudioConvertTool {
    fn descriptor(&self) -> ToolDescriptor {
        let mut base = AudioConvertTool.descriptor();
        base.id = "convert.audio".into();
        base.name = "Audio ⇄ Audio".into();
        base.name_key = "tool_conv_audio".into();
        base.category = ToolCategory::Convert;
        base.icon = "swap".into();
        base
    }

    fn validate(&self, inputs: &[PathBuf], options: &ToolOptions) -> Result<(), ToolError> {
        AudioConvertTool.validate(inputs, options)
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        AudioConvertTool.run(ctx)
    }
}

pub struct UnifiedVideoConvertTool;

impl Tool for UnifiedVideoConvertTool {
    fn descriptor(&self) -> ToolDescriptor {
        let mut base = VideoConvertTool.descriptor();
        base.id = "convert.video".into();
        base.name = "Video ⇄ Video".into();
        base.name_key = "tool_conv_video".into();
        base.category = ToolCategory::Convert;
        base.icon = "swap".into();
        base
    }

    fn validate(&self, inputs: &[PathBuf], options: &ToolOptions) -> Result<(), ToolError> {
        VideoConvertTool.validate(inputs, options)
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        VideoConvertTool.run(ctx)
    }
}

pub struct UnifiedPdfToImagesTool;

impl Tool for UnifiedPdfToImagesTool {
    fn descriptor(&self) -> ToolDescriptor {
        let mut base = PdfToImagesTool.descriptor();
        base.id = "convert.pdf_to_images".into();
        base.name = "PDF → Images".into();
        base.name_key = "tool_conv_pdf_img".into();
        base.category = ToolCategory::Convert;
        base.icon = "swap".into();
        base
    }

    fn validate(&self, inputs: &[PathBuf], options: &ToolOptions) -> Result<(), ToolError> {
        PdfToImagesTool.validate(inputs, options)
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        PdfToImagesTool.run(ctx)
    }
}

pub struct UnifiedImagesToPdfTool;

impl Tool for UnifiedImagesToPdfTool {
    fn descriptor(&self) -> ToolDescriptor {
        let mut base = ImagesToPdfTool.descriptor();
        base.id = "convert.images_to_pdf".into();
        base.name = "Images → PDF".into();
        base.name_key = "tool_conv_img_pdf".into();
        base.category = ToolCategory::Convert;
        base.icon = "swap".into();
        base
    }

    fn validate(&self, inputs: &[PathBuf], options: &ToolOptions) -> Result<(), ToolError> {
        ImagesToPdfTool.validate(inputs, options)
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        ImagesToPdfTool.run(ctx)
    }
}
