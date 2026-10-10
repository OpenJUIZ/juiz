
#include "juiz/juiz.h"
#include <iostream>
auto manifest() {
    return ProcessManifest{"cpp_talker"};
}

std::optional<std::string> cpp_talker() {
    std::cout << "cpp_talker() called" << std::endl;
    return "Hello, Juiz!";
}

PROCESS_FACTORY(manifest, cpp_talker);
