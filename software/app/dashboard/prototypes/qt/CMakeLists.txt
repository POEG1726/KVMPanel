cmake_minimum_required(VERSION 3.16)
project(proto_qt LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)

find_package(Qt6 6.4 REQUIRED COMPONENTS Core Gui Network Qml Quick)

add_executable(proto-qt
    main.cpp
    panelclient.h panelclient.cpp
    sparkline.h sparkline.cpp
    bloblayer.h bloblayer.cpp
    qml.qrc
)

target_link_libraries(proto-qt PRIVATE Qt6::Core Qt6::Gui Qt6::Network Qt6::Qml Qt6::Quick)
