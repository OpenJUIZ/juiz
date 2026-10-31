PROJECT_NAME=increment_process_cpp
DYLIB_EXT=dylib
TARGET=${JUIZ_LIB_DIR}/lib${PROJECT_NAME}.${DYLIB_EXT}

CC=clang
CXX=clang++
AR=ar


JUIZ_DIR=../../../..
JUIZ_LIB_DIR=${JUIZ_DIR}/target/debug
JUIZ_INCLUDE_DIR=${JUIZ_DIR}/bindings/cppjuiz/include
JUIZ_C_INCLUDE_DIR=${JUIZ_DIR}/bindings/cjuiz/include

CFLAGS=-Wall -O -I${JUIZ_INCLUDE_DIR} -I${JUIZ_C_INCLUDE_DIR} -c -fPIC -arch arm64 #-arch x86_64 -fPIC
CXXFLAGS=-Wall -O -I${JUIZ_INCLUDE_DIR} -I${JUIZ_C_INCLUDE_DIR} -c -fPIC -arch arm64 -std=c++20 #-arch x86_64 -fPIC
ARFLAGS= rc
CDYLIBFLAGS=-L${JUIZ_LIB_DIR} -ljuiz_core -dynamiclib -arch arm64 #-arch x86_64
CXXDYLIBFLAGS=-L${JUIZ_LIB_DIR} -ljuiz_core -dynamiclib -arch arm64 #-arch x86_64

all: ${TARGET}

.c.o:
	$(CC) $(CFLAGS) $<

.cpp.o:
	$(CXX) $(CXXFLAGS) $<

${JUIZ_LIB_DIR}/${PROJECT_NAME}.o: ${PROJECT_NAME}.cpp

	$(CXX) $(CXXFLAGS) $< -o $@

${TARGET}: ${JUIZ_LIB_DIR}/${PROJECT_NAME}.o 
	$(CXX) $(CXXDYLIBFLAGS) -o $@ $?

clean:
	rm -f *.o *.a *.dylib *~ ${TARGET} ${JUIZ_LIB_DIR}/${PROJECT_NAME}.o