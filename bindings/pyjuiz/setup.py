from setuptools import setup, find_packages

setup(
    name="pyjuiz",
    version="0.0.23",
    description='Python binding interface for JUIZ robot middleware components',
    url='https://github.com/OpenJUIZ/juiz',
    author='ysuga',
    author_email='ysuga@ysuga.net',
    license='MIT OR Apache-2.0',
    license_files=['LICENSE-MIT', 'LICENSE-APACHE'],
    keywords='robot,middleware',
    install_requires=["pyyaml", "httpx", "pillow", "docstring_parser"],
    extras_require={
        #"develop": ["dev-packageA", "dev-packageB"]
    },
    packages=find_packages(),
    classifiers=[
        'Programming Language :: Python :: 3.10',
        'License :: OSI Approved :: MIT License',
        'License :: OSI Approved :: Apache Software License',
    ],
)