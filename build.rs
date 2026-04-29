fn main() {
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("packaging/msix/Assets/RipMultiPaste.ico");
        resource.set_manifest_file("app.manifest");
        resource
            .compile()
            .expect("failed to embed Windows application resources");
    }
}
