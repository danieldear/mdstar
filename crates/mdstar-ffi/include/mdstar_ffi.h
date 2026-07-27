#ifndef MDSTAR_FFI_H
#define MDSTAR_FFI_H

#ifdef __cplusplus
extern "C" {
#endif

char *mdstar_render_html(const char *input);
char *mdstar_document_ir_json(const char *input, const char *origin);
char *mdstar_document_ir_from_file_json(const char *path);
char *mdstar_workspace_tree_json(const char *root);
void mdstar_string_free(char *pointer);
void markdown_string_free(char *pointer);

#ifdef __cplusplus
}
#endif
#endif
