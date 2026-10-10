from setuptools import setup, find_packages

setup(
    name="pyjuiz",
    version="0.0.4",
    description='Python binding interface for JUIZ robot middleware components',
    url='https://github.com/juiz',
    author='ysuga',
    author_email='ysuga@ysuga.net',
    license='MIT',
    keywords='robot,middleware',
    install_requires=["pyyaml", "httpx", "pillow"],
    extras_require={
        #"develop": ["dev-packageA", "dev-packageB"]
    },
    packages=find_packages(),
    classifiers=[
        'Programming Language :: Python :: 3.10',
    ],
)