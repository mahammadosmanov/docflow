#import <Foundation/Foundation.h>
#import <PDFKit/PDFKit.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>

int docflow_pdf_first_page_text(const char *path, char **output) {
    if (path == NULL || output == NULL) {
        return -1;
    }
    *output = NULL;
    @autoreleasepool {
        NSString *filePath = [NSString stringWithUTF8String:path];
        if (filePath == nil) {
            return -1;
        }
        PDFDocument *document = [[PDFDocument alloc]
            initWithURL:[NSURL fileURLWithPath:filePath]];
        if (document == nil || document.isLocked || document.pageCount == 0) {
            return -2;
        }
        PDFPage *page = [document pageAtIndex:0];
        NSString *text = page.string;
        if (text == nil) {
            text = @"";
        }
        if ([text lengthOfBytesUsingEncoding:NSUTF8StringEncoding] > 1048576) {
            return -5;
        }
        const char *utf8 = text.UTF8String;
        if (utf8 == NULL) {
            return -3;
        }
        *output = strdup(utf8);
        return *output == NULL ? -4 : 0;
    }
}

void docflow_pdfkit_free(char *value) {
    free(value);
}

int docflow_pdf_render_first_page(const char *input, const char *output, int dpi) {
    if (input == NULL || output == NULL || dpi < 72 || dpi > 600) {
        return -1;
    }
    @autoreleasepool {
        NSString *inputPath = [NSString stringWithUTF8String:input];
        NSString *outputPath = [NSString stringWithUTF8String:output];
        PDFDocument *document = [[PDFDocument alloc]
            initWithURL:[NSURL fileURLWithPath:inputPath]];
        if (document == nil || document.isLocked || document.pageCount == 0) {
            return -2;
        }
        PDFPage *page = [document pageAtIndex:0];
        NSRect bounds = [page boundsForBox:kPDFDisplayBoxCropBox];
        CGFloat scale = ((CGFloat)dpi) / 72.0;
        NSSize size = NSMakeSize(MAX(1.0, bounds.size.width * scale),
                                 MAX(1.0, bounds.size.height * scale));
        if (!isfinite(size.width) || !isfinite(size.height) ||
            size.width > 20000.0 || size.height > 20000.0 ||
            size.width * size.height > 100000000.0) {
            return -5;
        }
        NSImage *image = [page thumbnailOfSize:size forBox:kPDFDisplayBoxCropBox];
        NSData *tiff = image.TIFFRepresentation;
        if (tiff == nil) {
            return -3;
        }
        NSBitmapImageRep *bitmap = [NSBitmapImageRep imageRepWithData:tiff];
        NSData *png = [bitmap representationUsingType:NSBitmapImageFileTypePNG
                                           properties:@{}];
        if (png == nil || ![png writeToFile:outputPath atomically:YES]) {
            return -4;
        }
        return 0;
    }
}

int docflow_pdf_from_png(const char *input, const char *output) {
    if (input == NULL || output == NULL) {
        return -1;
    }
    @autoreleasepool {
        NSString *inputPath = [NSString stringWithUTF8String:input];
        NSString *outputPath = [NSString stringWithUTF8String:output];
        NSImage *image = [[NSImage alloc] initWithContentsOfFile:inputPath];
        if (image == nil) {
            return -2;
        }
        NSSize fixtureSize = NSMakeSize(612.0, 792.0);
        NSImage *pageImage = [[NSImage alloc] initWithSize:fixtureSize];
        [pageImage lockFocus];
        [image drawInRect:NSMakeRect(0.0, 0.0, fixtureSize.width, fixtureSize.height)
                 fromRect:NSZeroRect
                operation:NSCompositingOperationCopy
                 fraction:1.0];
        [pageImage unlockFocus];
        PDFPage *page = [[PDFPage alloc] initWithImage:pageImage];
        if (page == nil) {
            return -3;
        }
        PDFDocument *document = [[PDFDocument alloc] init];
        [document insertPage:page atIndex:0];
        return [document writeToURL:[NSURL fileURLWithPath:outputPath]] ? 0 : -4;
    }
}
