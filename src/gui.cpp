#include <QApplication>
#include <QComboBox>
#include <QCoreApplication>
#include <QFileDialog>
#include <QFormLayout>
#include <QLabel>
#include <QLineEdit>
#include <QProcess>
#include <QPushButton>
#include <QWidget>

int main(int argc, char** argv) {
    QApplication app(argc,argv);
    QWidget window;
    window.setWindowTitle("Deterministic Media Enhancer");
    auto* layout = new QFormLayout(&window);
    auto* input = new QLineEdit;
    auto* output = new QLineEdit;
    auto* open = new QPushButton("Select input");
    auto* save = new QPushButton("Select output");
    auto* scale = new QComboBox;
    scale->addItems({"1","2","4","8"});
    scale->setCurrentText("2");
    auto* preset = new QComboBox;
    preset->addItems({"clean","detail","resize"});
    auto* run = new QPushButton("Enhance");
    auto* status = new QLabel("Ready — local CPU processing");
    status->setWordWrap(true);
    auto* process = new QProcess(&window);
    layout->addRow(open,input);
    layout->addRow(save,output);
    layout->addRow("Scale",scale);
    layout->addRow("Preset",preset);
    layout->addRow(run);
    layout->addRow(status);
    QObject::connect(open,&QPushButton::clicked,[&]{
        const auto path=QFileDialog::getOpenFileName(&window,"Input media");
        if(!path.isEmpty()) input->setText(path);
    });
    QObject::connect(save,&QPushButton::clicked,[&]{
        const auto path=QFileDialog::getSaveFileName(&window,"Output media",{},"PNG (*.png);;MKV (*.mkv);;MP4 (*.mp4)");
        if(!path.isEmpty()) output->setText(path);
    });
    QObject::connect(run,&QPushButton::clicked,[&]{
        run->setEnabled(false); status->setText("Processing...");
        process->start(QCoreApplication::applicationDirPath()+"/dme",
            {input->text(),output->text(),scale->currentText(),preset->currentText()});
    });
    QObject::connect(process,&QProcess::finished,[&](int code,QProcess::ExitStatus result){
        run->setEnabled(true);
        status->setText(code==0 && result==QProcess::NormalExit ? "Completed" :
            "Failed: "+QString::fromUtf8(process->readAllStandardError()));
    });
    QObject::connect(process,&QProcess::errorOccurred,[&](QProcess::ProcessError){
        run->setEnabled(true); status->setText(process->errorString());
    });
    window.resize(650,240); window.show();
    return app.exec();
}
