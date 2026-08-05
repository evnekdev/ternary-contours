#include "main_window.hpp"

#include <QApplication>

int main(int argc, char* argv[]) {
    QApplication application(argc, argv);
    application.setApplicationName("Ternary Contours Qt");
    application.setOrganizationName("evnekdev");
    MainWindow window;
    window.show();
    return application.exec();
}