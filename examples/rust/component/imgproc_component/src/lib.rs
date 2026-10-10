
pub mod imgproc_component {
    use juiz_core::env_logger;
    use juiz_core::image::DynamicImage;
    use juiz_core::prelude::*;
    use juiz_core::image;

    #[no_mangle]
    pub unsafe extern "Rust" fn component_manifest() -> Value {
        let _ = env_logger::try_init();
        return jvalue!({
            "type_name": "imgproc_component",
            "containers": [
                {
                    "type_name": "imgproc_component_container",
                    "factory": "imgproc_component_container_factory",
                    "processes": [ 
                        {
                            "type_name": "imgproc_component_container_capture_image",
                            "factory": "imgproc_component_container_capture_image_factory",
                        },
                        {
                            "type_name": "example_component_container_get_image",
                            "factory": "example_component_container_get_image_factory"
                        }
                    ]
                }
            ],
            // "processes": [
            //     {
            //         "type_name": "increment_process",
            //         "factory": "increment_process_factory",
            //     }
            // ]
        }); 
    }

    #[repr(Rust)]
    pub struct ImageProcComponentContainer {
        image : DynamicImage,
    }

    impl ImageProcComponentContainer {

        pub fn manifest() -> Value {
            ContainerManifest::new("imageproc_component_container").into()
        }
    }

    fn create_imgproc_component_container(_manifest: Value) -> JuizResult<Box<ImageProcComponentContainer>> {
        Ok(Box::new(ImageProcComponentContainer{image: DynamicImage::new()}))
    }

    #[no_mangle]
    pub unsafe extern "Rust" fn imgproc_component_container_factory() -> JuizResult<ContainerFactoryPtr> {
        ContainerFactoryImpl::create(ImageProcComponentContainer::manifest(), create_imgproc_component_container)
    }

    

    fn imgproc_component_container_capture_function(container: &mut ContainerImpl<ImageProcComponentContainer>, _v: CapsuleMap) -> JuizResult<Capsule> {
        // Ok(jvalue!(container.value).into())
        // 
        todo!()
    }
    
    #[no_mangle]
    pub unsafe extern "Rust" fn imgproc_component_container_capture_factory() -> JuizResult<ContainerProcessFactoryPtr> {
        ContainerProcessFactoryImpl::create(
            ContainerProcessManifest::new(ImageProcComponentContainer::manifest(), "imgproc_component_container_capture").into(),
            &imgproc_component_container_capture_function)
    }
    
    fn imgproc_component_container_get_function(container: &mut ContainerImpl<ImageProcComponentContainer>, _v: CapsuleMap) -> JuizResult<Capsule> {
        // Ok(jvalue!(container.value).into())
        // 
        todo!()
    }
    
    #[no_mangle]
    pub unsafe extern "Rust" fn imgproc_component_container_get_factory() -> JuizResult<ContainerProcessFactoryPtr> {
        ContainerProcessFactoryImpl::create(
            ContainerProcessManifest::new(ImageProcComponentContainer::manifest(), "imgproc_component_container_get").into(),
            &imgproc_component_container_get_function)
    }


}