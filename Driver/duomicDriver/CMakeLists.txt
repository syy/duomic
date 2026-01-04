# duomic - Stereo USB Microphone Splitter Driver
cmake_minimum_required(VERSION 3.12.0)

project(duomicDriver CXX)

# Compiler options
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

# Codesign ID (leave empty for no signing)
set(CODESIGN_ID "" CACHE STRING "Codesign ID")

# Driver configuration
set(DRIVER_NAME "duomicDriver")
set(DRIVER_VERSION "0.1.0")
set(DRIVER_COPYRIGHT "Copyright (c) 2024 duomic")
set(DRIVER_IDENTIFIER "com.duomic.driver")
set(DRIVER_UID "D8F3E251-4B7A-4C9E-8F1D-2A3B4C5D6E7F")
set(DRIVER_ENTRYPOINT "DuomicDriverEntryPoint")

# Report configuration
message(STATUS "Driver name - ${DRIVER_NAME}")
message(STATUS "Driver version - ${DRIVER_VERSION}")
message(STATUS "Driver identifier - ${DRIVER_IDENTIFIER}")

# Path to libASPL source directory
get_filename_component(LIBASPL_SOURCE_DIR
  ${CMAKE_CURRENT_LIST_DIR}/../libASPL
  ABSOLUTE)

# Add rules for building libASPL
set(LIBASPL_TARGET duomicDriver_libASPL)
include(ExternalProject)
ExternalProject_Add(${LIBASPL_TARGET}
  SOURCE_DIR ${LIBASPL_SOURCE_DIR}
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/libASPL-build
  INSTALL_DIR ${CMAKE_CURRENT_BINARY_DIR}/libASPL-prefix
  CMAKE_ARGS -DCMAKE_INSTALL_PREFIX=<INSTALL_DIR>
  LOG_DOWNLOAD YES
  LOG_CONFIGURE YES
  LOG_BUILD YES
  LOG_INSTALL YES
  )

# Create MODULE library (Mach-O loadable bundle)
add_library(${DRIVER_NAME} MODULE
  "Driver.cpp"
  )

# Add libASPL dependency
add_dependencies(${DRIVER_NAME} ${LIBASPL_TARGET})
target_include_directories(${DRIVER_NAME}
  PRIVATE ${CMAKE_CURRENT_BINARY_DIR}/libASPL-prefix/include
  )
target_link_libraries(${DRIVER_NAME}
  PRIVATE ${CMAKE_CURRENT_BINARY_DIR}/libASPL-prefix/lib/libASPL.a
  )

# Add CoreFoundation and CoreAudio dependencies
find_library(LIB_CoreFoundation CoreFoundation REQUIRED)
find_library(LIB_CoreAudio CoreAudio REQUIRED)
target_link_libraries(${DRIVER_NAME}
  PRIVATE ${LIB_CoreFoundation}
  PRIVATE ${LIB_CoreAudio}
  )

# Configure bundle properties
set_target_properties(${DRIVER_NAME} PROPERTIES
  OUTPUT_NAME "${DRIVER_NAME}"
  BUNDLE true
  BUNDLE_EXTENSION "driver"
  PREFIX ""
  SUFFIX ""
  MACOSX_BUNDLE ON
  MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/Info.plist.in"
  MACOSX_BUNDLE_BUNDLE_NAME "${DRIVER_NAME}"
  MACOSX_BUNDLE_BUNDLE_VERSION "${DRIVER_VERSION}"
  MACOSX_BUNDLE_COPYRIGHT "${DRIVER_COPYRIGHT}"
  MACOSX_BUNDLE_GUI_IDENTIFIER "${DRIVER_IDENTIFIER}"
  MACOSX_BUNDLE_SHORT_VERSION_STRING "${DRIVER_VERSION}"
  )

# Sign the bundle if CODESIGN_ID is provided
if(NOT CODESIGN_ID STREQUAL "")
  add_custom_command(
    TARGET ${DRIVER_NAME}
    POST_BUILD
    COMMENT
      "Signing ${DRIVER_NAME}.driver with ID ${CODESIGN_ID}"
    VERBATIM
    COMMAND
      codesign --force -s "${CODESIGN_ID}"
        "${CMAKE_CURRENT_BINARY_DIR}/${DRIVER_NAME}.driver"
    )
endif()
