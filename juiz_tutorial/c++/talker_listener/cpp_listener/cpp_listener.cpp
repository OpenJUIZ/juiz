
#include <iostream>
#include "juiz/juiz.h"


ProcessManifest manifest() {
    return ProcessManifest{"cpp_listener"}
        .description("c++ version listener process")
        .add_string_arg("arg1", "test_argument", "Hello, Juiz!");

}

std::optional<int64_t> cpp_listener(const std::string& msg) {
    //std::cout << "cpp_listener() called" << std::endl;
    // auto a = cm.get_string("arg1");
    std::cout << "cpp_listener:" << msg << std::endl;
    return 0;
}

PROCESS_FACTORY(manifest, cpp_listener);

