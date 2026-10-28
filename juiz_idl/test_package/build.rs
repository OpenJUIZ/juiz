fn main() {
    juiz_idl::build::configure()
        .avsc(true)
        .processes(true)
        .rust_skeleton(true)
        .compile(&[
            "idl/geometry.jidl",
            // juiz idl import-ros2 で ROS 2 の型から作ったもの (juiz_idl/tests/ros2_test.rs で作り直す)
            "idl/ros2/demo_msgs/msg/Sample.jidl",
            "idl/ros2/demo_msgs/msg/Reading.jidl",
            "idl/ros2/demo_msgs/msg/Mapped.jidl",
            "idl/ros2/demo_msgs/srv/AddTwoInts.jidl",
            "idl/ros2/demo_msgs/srv/Trigger2.jidl",
        ])
        .unwrap();
}
